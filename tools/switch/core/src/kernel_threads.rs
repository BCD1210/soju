//! Thread SVC register ABI and lifetime transitions.
use super::*;
impl Kernel {
    pub(super) fn dispatch_thread(
        &mut self,
        memory: &mut dyn GuestMemory,
        current: ThreadId,
        svc: u16,
        r: &[u64; 31],
        now: u64,
    ) -> Result<Dispatch, u64> {
        Ok(match svc {
            8 => {
                if self
                    .threads
                    .values()
                    .filter(|t| t.status != ThreadStatus::Exited)
                    .count()
                    >= MAX_THREADS
                {
                    return Err(LIMIT_REACHED);
                }
                if r[4] > 63
                    || self.policy.as_ref().is_some_and(|p| {
                        !(p.highest_priority..=p.lowest_priority).contains(&(r[4] as u32))
                    })
                {
                    return Err(INVALID_PRIORITY);
                }
                let core = match r[5] {
                    0..=3 => u32::try_from(r[5]).map_err(|_| INVALID_ENUM)?,
                    0xfffffffe | 0xffffffff | 0xfffffffffffffffe | u64::MAX => 0,
                    _ => return Err(INVALID_ENUM),
                };
                if self
                    .policy
                    .as_ref()
                    .is_some_and(|p| !(p.min_core..=p.max_core).contains(&core))
                {
                    return Err(INVALID_ENUM);
                }
                if !r[1].is_multiple_of(4) || !r[3].is_multiple_of(16) || r[3] < 16 {
                    return Err(INVALID_ADDRESS);
                }
                memory
                    .check_access(r[1], 4, 4)
                    .map_err(|_| INVALID_CURRENT_MEMORY)?;
                memory
                    .check_access(r[3] - 16, 16, 2)
                    .map_err(|_| INVALID_CURRENT_MEMORY)?;
                self.allocation_available()?;
                let id = self.next_handle;
                let slot = (0..MAX_THREADS)
                    .find(|slot| {
                        !self
                            .threads
                            .values()
                            .any(|t| t.spec.tls == 0x310000000 + (*slot * PAGE) as u64)
                    })
                    .ok_or(LIMIT_REACHED)?;
                let tls = 0x310000000 + (slot * PAGE) as u64;
                memory.map_zeroed(tls, PAGE, 3).map_err(|_| LIMIT_REACHED)?;
                memory
                    .set_state(tls, PAGE, 0xc)
                    .map_err(|_| INVALID_MEMORY_STATE)?;
                let handle = self.handle(Object::Thread(id))?;
                self.threads.insert(
                    id,
                    Thread {
                        spec: ThreadSpec {
                            entry: r[1],
                            argument: r[2],
                            stack: r[3],
                            tls,
                            link: 0,
                        },
                        status: ThreadStatus::Suspended,
                        priority: r[4] as u32,
                        ideal_core: i32::try_from(core).map_err(|_| INVALID_ENUM)?,
                        affinity: 1 << core,
                        deadline: None,
                        pending: None,
                        cancelled: false,
                    },
                );
                out(handle as u64)
            }
            9 => {
                let id = self
                    .resolve_thread(r[0] as u32, current)
                    .ok_or(INVALID_HANDLE)?;
                if self.threads[&id].status != ThreadStatus::Suspended {
                    return Err(INVALID_STATE);
                }
                self.threads.get_mut(&id).expect("resolved").status = ThreadStatus::Runnable;
                self.requeue(id);
                ret(0)
            }
            10 => {
                self.threads.get_mut(&current).expect("current").status = ThreadStatus::Exited;
                self.ready.retain(|id| *id != current);
                self.poll_objects();
                let object = self
                    .objects
                    .iter()
                    .find_map(|(id, e)| {
                        matches!(e.object,Object::Thread(t) if t==current).then_some(*id)
                    })
                    .ok_or(INVALID_HANDLE)?;
                self.release_object(memory, object)?;
                Dispatch::ThreadExited
            }
            11 => {
                if r[0] as i64 > 0 {
                    self.block(current, Wait::Sleep, deadline(now, r[0]))
                } else {
                    ret(0)
                }
            }
            12 => {
                let id = self
                    .resolve_thread(r[1] as u32, current)
                    .ok_or(INVALID_HANDLE)?;
                out(self.threads[&id].priority as u64)
            }
            13 => {
                if r[1] > 63
                    || self.policy.as_ref().is_some_and(|p| {
                        !(p.highest_priority..=p.lowest_priority).contains(&(r[1] as u32))
                    })
                {
                    return Err(INVALID_PRIORITY);
                }
                let id = self
                    .resolve_thread(r[0] as u32, current)
                    .ok_or(INVALID_HANDLE)?;
                self.threads.get_mut(&id).expect("resolved").priority = r[1] as u32;
                ret(0)
            }
            14 => {
                let id = self
                    .resolve_thread(r[2] as u32, current)
                    .ok_or(INVALID_HANDLE)?;
                let t = &self.threads[&id];
                Dispatch::Return(vec![0, t.ideal_core as u64, t.affinity])
            }
            15 => {
                let id = self
                    .resolve_thread(r[0] as u32, current)
                    .ok_or(INVALID_HANDLE)?;
                let allowed = self.policy.as_ref().map_or(15, |p| {
                    ((1u64 << (p.max_core + 1)) - 1) & !((1u64 << p.min_core) - 1)
                });
                let ideal = match r[1] {
                    0..=3 => i32::try_from(r[1]).map_err(|_| INVALID_ENUM)?,
                    0xffffffff | u64::MAX => -1,
                    0xfffffffe | 0xfffffffffffffffe => 0,
                    _ => return Err(INVALID_ENUM),
                };
                if r[2] == 0 || r[2] & !allowed != 0 || (ideal >= 0 && r[2] & (1u64 << ideal) == 0)
                {
                    return Err(INVALID_COMBINATION);
                }
                let t = self.threads.get_mut(&id).expect("resolved");
                t.ideal_core = ideal;
                t.affinity = r[2];
                ret(0)
            }
            16 => {
                let t = &self.threads[&current];
                ret(if t.ideal_core >= 0 {
                    u64::try_from(t.ideal_core).map_err(|_| INVALID_ENUM)?
                } else {
                    u64::from(t.affinity.trailing_zeros())
                })
            }
            _ => return Err(INVALID_ENUM),
        })
    }
}
