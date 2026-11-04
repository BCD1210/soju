#[path = "common/program.rs"]
mod fixture;
use soju_switch_core::{
    kernel::{Dispatch, Kernel, MAIN_THREAD},
    memory::MemoryImage,
    nro::{BASE, STACK},
    program::Program,
};
#[test]
fn loads_own_nsp_and_xci_modules_policy_and_romfs() {
    for ext in ["nsp", "xci"] {
        let (bytes, keys, romfs) = fixture::package(ext, true);
        let program = Program::load(&bytes, ext, &keys).unwrap();
        assert_eq!(
            program
                .modules
                .iter()
                .map(|m| m.0.as_str())
                .collect::<Vec<_>>(),
            ["rtld", "main", "subsdk0", "sdk"]
        );
        assert_eq!(program.modules[1].1, BASE + 0x3000);
        assert_eq!(program.boot.pc, BASE);
        assert_eq!(program.boot.sp, STACK + 0x100000);
        assert_eq!(program.romfs, romfs);
        let mut memory = MemoryImage::new(&program.boot).unwrap();
        assert!(memory.check_access(BASE, 4, 4).is_ok());
        assert!(memory.check_access(BASE, 4, 2).is_err());
        let mut kernel = Kernel::new(&program.boot);
        kernel.configure_program(&program);
        let main = kernel.thread(MAIN_THREAD).unwrap();
        assert_eq!((main.priority, main.ideal_core, main.affinity), (35, 2, 4));
        assert!(matches!(
            kernel.dispatch(&mut memory, MAIN_THREAD, 7, &[0; 31], BASE, 0),
            Dispatch::ProcessExited(0)
        ));
        assert!(matches!(
            kernel.dispatch(&mut memory, MAIN_THREAD, 1, &[0; 31], BASE, 0),
            Dispatch::Unsupported(1)
        ));
    }
}
#[test]
fn missing_keys_and_program_are_explicit_errors() {
    let (bytes, _, _) = fixture::package("nsp", true);
    assert!(Program::load(&bytes, "nsp", &Default::default()).is_err());
    assert!(
        Program::load(
            &soju_switch_fs::pfs0::build(&[]),
            "nsp",
            &Default::default()
        )
        .is_err()
    );
}

#[test]
fn rejects_unsupported_or_inconsistent_npdm() {
    let mutations: [fn(&mut Vec<u8>); 4] = [
        |b| b[12] = 5,
        |b| b[15] = 4,
        |b| b[28] = 1,
        |b| {
            let n = b.len();
            b[n - 4..].copy_from_slice(&0x7fu32.to_le_bytes());
        },
    ];
    for edit in mutations {
        let (bytes, keys, _) = fixture::package_with_metadata("nsp", true, edit);
        assert!(Program::load(&bytes, "nsp", &keys).is_err());
    }
}
