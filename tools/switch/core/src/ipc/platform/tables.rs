use super::*;
const fn c(
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
                    c(5, 0, zero),
                    c(6, 0, zero),
                    c(9, 0, one),
                ]
            }
        }
        Kind::SelfController => {
            const {
                &[
                    c(9, 0, Platform::signaled_event),
                    c(11, 1, success),
                    c(12, 1, success),
                    c(40, 0, one),
                    c(91, 0, Platform::event),
                ]
            }
        }
        Kind::Window => const { &[c(1, 0, one)] },
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
        Kind::Timezone => const { &[c(101, 8, time::calendar)] },
        Kind::Account => const { &[c(50, 0, success)] },
        Kind::Apm => const { &[c(0, 0, |s, _, x| s.child(Kind::Empty, x)), c(1, 0, zero)] },
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
        Kind::Bsd => const { &[c(0, 0, success), c(1, 0, success)] },
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
    c(20, 0, |s, _, x| s.child(Kind::Empty, x)),
    c(1000, 0, |s, _, x| s.child(Kind::Empty, x)),
];
const HID: &[CommandSpec<Platform>] = &[
    c(0, 8, |s, _, x| s.child(Kind::HidResource, x)),
    c(11, 8, success),
    c(100, 16, success),
    c(102, 8, success),
    c(103, 8, success),
    c(109, 16, success),
    c(124, 8, success),
    c(201, 24, success),
    c(203, 0, |s, _, x| s.child(Kind::Vibration, x)),
    c(204, 1, success),
    c(205, 0, zero),
    c(206, 8, success),
];
