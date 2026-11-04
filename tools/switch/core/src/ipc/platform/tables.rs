use super::*;
pub(super) const fn c(
    id: u32,
    n: usize,
    handler: fn(&mut Platform, &Request, &mut Context<'_>) -> Result<Reply, Error>,
) -> CommandSpec<Platform> {
    CommandSpec {
        id,
        schema: Schema::at_least(n),
        handler,
    }
}
pub(super) fn table(kind: Kind) -> &'static [CommandSpec<Platform>] {
    match kind {
        Kind::Empty => &[],
        Kind::PlayHistory => play_history::COMMANDS,
        Kind::Ns => const { &[c(7989, 0, |s, _, x| s.child(Kind::ControlData, x))] },
        Kind::ControlData => ns::CONTROL,
        Kind::Usb => usb::COMMANDS,
        // The desktop profile is permanently on mains power with full charge.
        Kind::Power => {
            const {
                &[
                    c(0, 0, |_, _, _| {
                        Ok(Reply::data(100u32.to_le_bytes().to_vec()))
                    }),
                    c(1, 0, |_, _, _| Ok(Reply::data(1u32.to_le_bytes().to_vec()))),
                    c(13, 0, |_, _, _| {
                        Ok(Reply::data(100f64.to_le_bytes().to_vec()))
                    }),
                    c(14, 0, |_, _, _| Ok(Reply::data(vec![1]))),
                    c(15, 0, |_, _, _| {
                        Ok(Reply::data(100f64.to_le_bytes().to_vec()))
                    }),
                ]
            }
        }
        Kind::Settings => settings::COMMANDS,
        Kind::Firmware => const { &[c(3, 0, firmware), c(4, 0, firmware)] },
        Kind::Ae => {
            const {
                &[
                    c(200, 8, |s, _, x| s.child(Kind::LibraryProxy, x)),
                    c(201, 8, |s, _, x| s.child(Kind::LibraryProxy, x)),
                ]
            }
        }
        Kind::Oe => const { &[c(0, 8, |s, _, x| s.child(Kind::ApplicationProxy, x))] },
        Kind::LibraryProxy => LIBRARY_PROXY,
        Kind::ApplicationProxy => APPLICATION_PROXY,
        Kind::Common => {
            const {
                &[
                    c(0, 0, Platform::message_event),
                    c(1, 0, Platform::message),
                    c(5, 0, operation_mode),
                    c(6, 0, operation_mode),
                    c(9, 0, one),
                ]
            }
        }
        Kind::SelfController => {
            const {
                &[
                    c(1, 0, |s, _, _| {
                        s.exit_locked = true;
                        Ok(Reply::success())
                    }),
                    c(2, 0, |s, _, _| {
                        s.exit_locked = false;
                        Ok(Reply::success())
                    }),
                    c(9, 0, Platform::signaled_event),
                    c(10, 4, success),
                    c(11, 1, success),
                    c(12, 1, success),
                    c(13, 3, success),
                    c(14, 1, success),
                    c(16, 1, success),
                    c(40, 0, one),
                    c(91, 0, Platform::event),
                ]
            }
        }
        Kind::Window => const { &[c(1, 0, one), c(10, 0, success)] },
        Kind::Winding => const { &[c(0, 0, zero)] },
        Kind::LibrarySelf => const { &[c(11, 0, zero)] },
        Kind::Creator => {
            const {
                &[
                    c(0, 8, Platform::create_player),
                    c(10, 8, Platform::create_storage),
                ]
            }
        }
        Kind::Storage => const { &[c(0, 0, Platform::open_storage)] },
        Kind::Accessor => {
            const {
                &[
                    c(0, 0, Platform::storage_size),
                    c(10, 8, Platform::storage_write),
                    c(11, 8, Platform::storage_read),
                ]
            }
        }
        Kind::Player => {
            const {
                &[
                    c(0, 0, Platform::player_event),
                    c(10, 0, Platform::player_start),
                    c(30, 0, Platform::player_result),
                    c(100, 0, Platform::player_push),
                    c(101, 0, Platform::player_pop),
                ]
            }
        }
        Kind::Hid => HID,
        Kind::HidSystem => hid_system::COMMANDS,
        Kind::HidResource => const { &[c(0, 0, Platform::hid_shared)] },
        Kind::Vibration => const { &[c(0, 4, success)] },
        Kind::Time => {
            const {
                &[
                    c(0, 0, clock_child),
                    c(1, 0, clock_child),
                    c(2, 0, clock_child),
                    c(3, 0, |s, _, x| s.child(Kind::Timezone, x)),
                    c(4, 0, |s, _, x| s.child(Kind::Empty, x)),
                    c(20, 0, Platform::time_shared),
                ]
            }
        }
        Kind::Clock => const { &[c(0, 0, Platform::clock_read)] },
        Kind::Timezone => {
            const {
                &[
                    c(0, 0, time::location),
                    c(4, 36, time::load_rule),
                    c(100, 8, time::calendar),
                    c(101, 8, time::calendar),
                ]
            }
        }
        Kind::Account => account::COMMANDS,
        Kind::Profile => account::PROFILE,
        Kind::Apm => {
            const {
                &[
                    c(0, 0, |s, _, x| s.child(Kind::ApmSession, x)),
                    c(1, 0, operation_mode),
                ]
            }
        }
        Kind::ApmSession => {
            const {
                &[
                    c(0, 8, |s, r, _| {
                        let mode = r.u32(0)? as usize;
                        let config = r.u32(4)?;
                        *s.performance
                            .get_mut(mode)
                            .ok_or(Error::Malformed("performance mode"))? = config;
                        Ok(Reply::success())
                    }),
                    c(1, 4, |s, r, _| {
                        let config = s
                            .performance
                            .get(r.u32(0)? as usize)
                            .ok_or(Error::Malformed("performance mode"))?;
                        Ok(Reply::data(config.to_le_bytes().to_vec()))
                    }),
                ]
            }
        }
        Kind::ApplicationFunctions => {
            const {
                &[
                    c(1, 4, Platform::pop_launch),
                    c(21, 0, |_, _, _| Ok(Reply::data(b"en-US\0\0\0".to_vec()))),
                    c(40, 0, one),
                ]
            }
        }
        Kind::Friends => const { &[c(0, 0, |s, _, x| s.child(Kind::FriendService, x))] },
        Kind::FriendService => const { &[c(0, 0, Platform::event)] },
        Kind::ParentalFactory => {
            const {
                &[
                    c(0, 8, |s, _, x| s.child(Kind::Parental, x)),
                    c(1, 8, |s, _, x| s.child(Kind::Parental, x)),
                ]
            }
        }
        Kind::Prepo => const { &[c(10300, 0, zero)] },
        Kind::Arp => {
            const {
                &[
                    c(1, 8, |s, r, _| {
                        if r.u64(0)? != 1 {
                            return Err(Error::Malformed("application instance"));
                        }
                        let bytes = s
                            .control
                            .as_ref()
                            .ok_or(Error::Malformed("missing application control property"))?;
                        Reply::success().write(r.output()?, bytes.clone())
                    }),
                    c(3, 8, |_, r, _| {
                        if r.u64(0)? != 1 {
                            return Err(Error::Malformed("application process"));
                        }
                        Ok(Reply::data(1u64.to_le_bytes().to_vec()))
                    }),
                ]
            }
        }
        Kind::Parental => const { &[c(1, 0, success), c(1031, 0, zero)] },
        Kind::Fonts => {
            const {
                &[
                    c(0, 4, Platform::font_request),
                    c(1, 4, Platform::font_state),
                    c(2, 4, Platform::font_size),
                    c(3, 4, Platform::font_offset),
                    c(4, 0, Platform::font_shared),
                    c(5, 8, Platform::font_list),
                ]
            }
        }
        Kind::Bsd => network::BSD,
        Kind::Ssl => network::SSL,
        Kind::SslContext => const { &[c(3, 0, zero)] },
        Kind::Dns => network::DNS,
        Kind::Nifm => {
            const {
                &[
                    c(4, 0, |s, _, c| s.child(Kind::Network, c)),
                    c(5, 8, |s, _, c| s.child(Kind::Network, c)),
                ]
            }
        }
        Kind::Network => {
            const {
                &[
                    c(5, 0, network::unavailable),
                    c(12, 0, network::unavailable),
                    c(17, 0, zero),
                    c(18, 0, zero),
                    c(21, 0, zero),
                    c(22, 0, zero),
                ]
            }
        }
        Kind::Random => const { &[c(0, 0, network::random)] },
    }
}
fn clock_child(s: &mut Platform, _: &Request, x: &mut Context<'_>) -> Result<Reply, Error> {
    s.child(Kind::Clock, x)
}
const LIBRARY_PROXY: &[CommandSpec<Platform>] = &[
    c(0, 0, |s, _, x| s.child(Kind::Common, x)),
    c(1, 0, |s, _, x| s.child(Kind::SelfController, x)),
    c(2, 0, |s, _, x| s.child(Kind::Window, x)),
    c(3, 0, |s, _, x| s.child(Kind::Empty, x)),
    c(4, 0, |s, _, x| s.child(Kind::Empty, x)),
    c(10, 0, |s, _, x| s.child(Kind::Winding, x)),
    c(11, 0, |s, _, x| s.child(Kind::Creator, x)),
    c(20, 0, |s, _, x| s.child(Kind::LibrarySelf, x)),
    c(21, 0, |s, _, x| s.child(Kind::Empty, x)),
    c(22, 0, |s, _, x| s.child(Kind::Empty, x)),
    c(23, 0, |s, _, x| s.child(Kind::Empty, x)),
    c(1000, 0, |s, _, x| s.child(Kind::Empty, x)),
];
// Switchbrew AM_services IApplicationProxy (HOS 17): cmd20 is ApplicationFunctions.
const APPLICATION_PROXY: &[CommandSpec<Platform>] = &[
    c(0, 0, |s, _, x| s.child(Kind::Common, x)),
    c(1, 0, |s, _, x| s.child(Kind::SelfController, x)),
    c(2, 0, |s, _, x| s.child(Kind::Window, x)),
    c(3, 0, |s, _, x| s.child(Kind::Empty, x)),
    c(4, 0, |s, _, x| s.child(Kind::Empty, x)),
    c(10, 0, |s, _, x| s.child(Kind::Winding, x)),
    c(11, 0, |s, _, x| s.child(Kind::Creator, x)),
    c(20, 0, |s, _, x| s.child(Kind::ApplicationFunctions, x)),
    c(1000, 0, |s, _, x| s.child(Kind::Empty, x)),
];
const HID: &[CommandSpec<Platform>] = &[
    c(0, 8, |s, _, x| s.child(Kind::HidResource, x)),
    c(11, 8, success),
    c(21, 8, success),
    c(31, 8, success),
    c(66, 16, |s, r, _| {
        let handle = r.u32(0)?;
        shared::sensor_offset(handle)?;
        if !s.six_axis.contains(&handle) && s.six_axis.len() >= 64 {
            return Err(Error::Malformed("six-axis handle limit"));
        }
        s.six_axis.insert(handle);
        Ok(Reply::success())
    }),
    c(67, 16, |s, r, _| {
        shared::sensor_offset(r.u32(0)?)?;
        s.six_axis.remove(&r.u32(0)?);
        Ok(Reply::success())
    }),
    c(82, 16, |_, r, _| {
        shared::sensor_offset(r.u32(0)?)?;
        Ok(Reply::data(1u64.to_le_bytes().to_vec()))
    }),
    c(100, 16, |s, r, _| {
        s.npad_styles = r.u32(0)?;
        Ok(Reply::success())
    }),
    c(101, 8, |s, _, _| {
        Ok(Reply::data(s.npad_styles.to_le_bytes().to_vec()))
    }),
    c(102, 8, success),
    c(103, 8, success),
    c(109, 16, success),
    c(120, 16, |s, r, _| {
        let hold = r.u64(8)?;
        if hold > 1 {
            return Err(Error::Malformed("Npad hold type"));
        }
        s.joy_hold_type = hold;
        Ok(Reply::success())
    }),
    c(121, 8, |s, _, _| {
        Ok(Reply::data(s.joy_hold_type.to_le_bytes().to_vec()))
    }),
    c(124, 8, success),
    c(128, 16, |s, r, _| {
        let mode = r.u64(8)?;
        if mode > 2 {
            return Err(Error::Malformed("handheld activation mode"));
        }
        s.handheld_mode = mode;
        Ok(Reply::success())
    }),
    c(129, 8, |s, _, _| {
        Ok(Reply::data(s.handheld_mode.to_le_bytes().to_vec()))
    }),
    c(201, 24, success),
    c(203, 0, |s, _, x| s.child(Kind::Vibration, x)),
    c(204, 1, success),
    c(205, 0, zero),
    c(206, 8, success),
];

fn operation_mode(s: &mut Platform, _: &Request, _: &mut Context<'_>) -> Result<Reply, Error> {
    Ok(Reply::data(s.operation_mode.to_le_bytes().to_vec()))
}
