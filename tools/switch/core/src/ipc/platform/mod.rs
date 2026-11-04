//! Platform services backed by kernel objects and bounded host resources.
mod account;
mod applet;
#[cfg(test)]
mod boot_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod exit_tests;
mod fonts;
mod network;
mod settings;
mod shared;
#[cfg(test)]
mod shared_tests;
mod storage;
mod tables;
#[cfg(test)]
mod tests;
mod time;
use super::types::*;
use crate::kernel::{Handle, Kernel, ObjectId};
use crate::memory::GuestMemory;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    path::Path,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Empty,
    Firmware,
    Settings,
    Ae,
    Oe,
    LibraryProxy,
    ApplicationProxy,
    Common,
    SelfController,
    Window,
    Winding,
    LibrarySelf,
    Creator,
    Storage,
    Accessor,
    Player,
    Hid,
    HidResource,
    Vibration,
    Time,
    Clock,
    Timezone,
    Account,
    Profile,
    Apm,
    ApmSession,
    ApplicationFunctions,
    Friends,
    FriendService,
    ParentalFactory,
    Parental,
    Fonts,
    Bsd,
    Ssl,
    Dns,
    Random,
    Nifm,
    Network,
}
struct Object {
    kind: Kind,
    client: ClientId,
}
#[derive(Default)]
struct Player {
    inputs: Vec<Vec<u8>>,
    event: Option<Handle>,
    output: Option<Vec<u8>>,
    completed: bool,
}
pub struct Platform {
    objects: BTreeMap<ObjectId, Object>,
    storage: BTreeMap<ObjectId, Rc<RefCell<Vec<u8>>>>,
    players: BTreeMap<ObjectId, Player>,
    messages: VecDeque<u32>,
    message_events: Vec<Handle>,
    owned_events: BTreeMap<ObjectId, Vec<Handle>>,
    retired_events: Vec<Handle>,
    exit_requested: bool,
    exit_locked: bool,
    input: soju_switch_input::NpadState,
    sample: u64,
    joy_hold_type: u64,
    npad_styles: u32,
    handheld_mode: u64,
    six_axis: std::collections::BTreeSet<u32>,
    performance: [u32; 2],
    launch_popped: bool,
    hid: Option<Handle>,
    time: Option<Handle>,
    font_handle: Option<Handle>,
    fonts: Option<fonts::Fonts>,
    clock: time::Clock,
    timezone: time::Timezone,
}
impl Platform {
    pub fn new(home: &Path) -> Result<Self, Error> {
        Ok(Self {
            objects: BTreeMap::new(),
            storage: BTreeMap::new(),
            players: BTreeMap::new(),
            messages: VecDeque::new(),
            message_events: Vec::new(),
            owned_events: BTreeMap::new(),
            retired_events: Vec::new(),
            exit_requested: false,
            exit_locked: false,
            input: Default::default(),
            sample: 1,
            joy_hold_type: 0,
            npad_styles: 0,
            handheld_mode: 0,
            six_axis: Default::default(),
            performance: [0x20000, 0x10000],
            launch_popped: false,
            hid: None,
            time: None,
            font_handle: None,
            fonts: fonts::Fonts::load(home)?,
            clock: time::Clock::Host,
            timezone: time::Timezone::host()?,
        })
    }
    /// Select a reproducible wall-clock origin; `now_ns` remains the caller's monotonic clock.
    pub fn set_clock_epoch(&mut self, seconds: i64) {
        self.clock = time::Clock::Fixed(seconds);
    }
    pub fn open(&mut self, name: &str, ctx: &mut Context<'_>) -> Result<Handle, Error> {
        let kind = match name {
            "ns:am2" | "psm" => Kind::Empty,
            "set" => Kind::Settings,
            "set:sys" => Kind::Firmware,
            "appletAE" => Kind::Ae,
            "appletOE" => Kind::Oe,
            "hid" => Kind::Hid,
            "time:u" | "time:s" => Kind::Time,
            "acc:u0" | "acc:u1" => Kind::Account,
            "apm:p" | "apm" => Kind::Apm,
            "friend:u" => Kind::Friends,
            "pctl" | "pctl:a" => Kind::ParentalFactory,
            "pl:u" => Kind::Fonts,
            "bsd:s" | "bsd:u" => Kind::Bsd,
            "ssl" => Kind::Ssl,
            "sfdnsres" => Kind::Dns,
            "csrng" => Kind::Random,
            "nifm:u" => Kind::Nifm,
            _ => return Err(Error::Malformed("unknown platform service")),
        };
        self.session(kind, ctx)
    }
    fn session(&mut self, kind: Kind, ctx: &mut Context<'_>) -> Result<Handle, Error> {
        let (handle, id) = ctx.new_session(Family::Platform)?;
        self.objects.insert(
            id,
            Object {
                kind,
                client: ctx.client,
            },
        );
        Ok(handle)
    }
    fn child(&mut self, kind: Kind, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        Ok(Reply::object(self.session(kind, ctx)?))
    }
    pub fn dispatch(&mut self, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        let object = self
            .objects
            .get(&ctx.object)
            .ok_or(Error::Malformed("platform object"))?;
        if object.client != ctx.client {
            return Err(Error::Malformed("platform object owner"));
        }
        dispatch_table(self, tables::table(object.kind), req, ctx)
    }
    pub fn close_object(&mut self, id: ObjectId) {
        self.objects.remove(&id);
        self.storage.remove(&id);
        self.players.remove(&id);
        if let Some(events) = self.owned_events.remove(&id) {
            self.message_events.retain(|h| !events.contains(h));
            self.retired_events.extend(events);
        }
    }
    pub fn set_buttons(&mut self, mask: u64) {
        self.input = soju_switch_input::NpadState {
            buttons: mask,
            ..Default::default()
        };
    }
    pub fn set_input(&mut self, input: soju_switch_input::NpadState) {
        self.input = input;
    }
    pub fn poll(
        &mut self,
        kernel: &mut Kernel,
        memory: &mut dyn GuestMemory,
        now_ns: u64,
    ) -> Result<(), Error> {
        for handle in self.retired_events.drain(..) {
            kernel.close_handle(memory, handle).map_err(Error::Kernel)?;
        }
        self.update_shared(kernel, memory, now_ns)
    }
    pub fn exit_grace(&self) -> std::time::Duration {
        std::time::Duration::from_secs(if self.exit_locked { 15 } else { 5 })
    }
    pub fn request_exit(&mut self, kernel: &mut Kernel) -> Result<bool, Error> {
        if !self.exit_requested {
            for &event in &self.message_events {
                kernel.signal_event(event).map_err(Error::Kernel)?;
            }
            self.messages.push_back(4);
            self.exit_requested = true;
        }
        Ok(!self.message_events.is_empty())
    }
}
fn success(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::success())
}
fn zero(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(vec![0; 8]))
}
fn one(_: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(1u64.to_le_bytes().to_vec()))
}
fn firmware(_: &mut Platform, req: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    let output = req.buffer(BufferKind::ReceiveList, 0)?;
    if output.len != 256 {
        return Err(Error::Malformed("firmware output size"));
    }
    let mut bytes = vec![0; 256];
    bytes[0] = 17;
    Reply::success().write(output, bytes)
}
