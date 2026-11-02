// ABOUTME: Exercises production MP3 linker-script placement with real files.
// ABOUTME: Checks narrow selectors and fails closed on ambiguous build inputs.
#[path = "../../firmware/build_support/mp3_iram.rs"]
mod mp3_iram;

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
const ANCHOR: &str = "    _iram_text_start = ABSOLUTE(.);\n";
const MEMBERS: [&str; 10] = [
    "bitstream",
    "huffman",
    "l3bitstream",
    "l3loop",
    "l3mdct",
    "l3subband",
    "layer3",
    "reservoir",
    "tables",
    "bridge",
];
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "mp3-linker-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("ld")).unwrap();
        fs::create_dir(root.join("out")).unwrap();
        Self(root)
    }
    fn script(&self, text: &str) {
        fs::write(self.0.join("ld/sections.ld"), text).unwrap();
    }
    fn args(&self) -> Vec<String> {
        vec![
            "--ldproxy-cwd".into(),
            self.0.display().to_string(),
            "@toolchain/ldflags".into(),
            "-T".into(),
            "memory.ld".into(),
            "-T".into(),
            "sections.ld".into(),
            "-Lld".into(),
        ]
    }
    fn place(&self, args: &mut [String]) -> Result<PathBuf, String> {
        mp3_iram::place_mp3(args, &self.0.join("out"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn derives_only_c_instruction_selectors_and_replaces_only_sections_argument() {
    for split_search_path in [false, true] {
        let fixture = Fixture::new();
        let original = format!("SECTIONS {{\n{ANCHOR}    *(.iram1 .iram1.*)\n}}\n");
        fixture.script(&original);
        let mut args = fixture.args();
        if split_search_path {
            args.pop();
            args.extend(["-L".into(), "ld".into()]);
        }
        let before = args.clone();
        assert_eq!(
            fixture.place(&mut args).unwrap(),
            fixture.0.join("ld/sections.ld").canonicalize().unwrap()
        );
        let output = fixture.0.join("out/sections-mp3-iram.ld");
        let derived = fs::read_to_string(&output).unwrap();
        let selectors: String = MEMBERS.iter().map(|member| format!("    *libesp_shazam_mp3-*.rlib:*-{member}.o(.literal .literal.* .text .text.*)\n")).collect();
        assert_eq!(
            derived,
            original.replacen(ANCHOR, &format!("{ANCHOR}{selectors}"), 1)
        );
        assert_eq!(
            fs::read_to_string(fixture.0.join("ld/sections.ld")).unwrap(),
            original
        );
        for (i, arg) in args.iter().enumerate() {
            assert_eq!(
                arg,
                if i == 6 {
                    output.to_str().unwrap()
                } else {
                    &before[i]
                }
            );
        }
        assert!(!derived.contains("rcgu"));
        assert!(!selectors.contains(".rodata"));
        assert!(!selectors.contains(".data"));
    }
}

#[test]
fn rejects_missing_or_duplicate_sections_link_arguments_without_mutating() {
    let fixture = Fixture::new();
    fixture.script(ANCHOR);
    for mut args in [vec![], vec!["-T".into()], {
        let mut a = fixture.args();
        a.extend(["-T".into(), "sections.ld".into()]);
        a
    }] {
        let before = args.clone();
        assert!(fixture
            .place(&mut args)
            .unwrap_err()
            .contains("exactly one -T sections.ld"));
        assert_eq!(args, before);
    }
}

#[test]
fn rejects_missing_or_ambiguous_script_and_anchor_without_writing_output() {
    let fixture = Fixture::new();
    let mut args = fixture.args();
    assert!(fixture
        .place(&mut args)
        .unwrap_err()
        .contains("exactly one sections.ld"));
    fixture.script(ANCHOR);
    fs::create_dir(fixture.0.join("other")).unwrap();
    fs::write(fixture.0.join("other/sections.ld"), ANCHOR).unwrap();
    let mut ambiguous = args.clone();
    ambiguous.push("-Lother".into());
    assert!(fixture
        .place(&mut ambiguous)
        .unwrap_err()
        .contains("exactly one sections.ld"));
    for content in ["SECTIONS {}".to_string(), ANCHOR.repeat(2)] {
        fixture.script(&content);
        let before = args.clone();
        assert!(fixture
            .place(&mut args)
            .unwrap_err()
            .contains("exactly one IRAM anchor"));
        assert_eq!(args, before);
        assert_eq!(
            fs::read_to_string(fixture.0.join("ld/sections.ld")).unwrap(),
            content
        );
    }
    assert!(!fixture.0.join("out/sections-mp3-iram.ld").exists());
}

#[test]
fn rejects_missing_or_ambiguous_linker_working_directory() {
    let fixture = Fixture::new();
    fixture.script(ANCHOR);
    for mut args in [vec!["-T".into(), "sections.ld".into(), "-Lld".into()], {
        let mut a = fixture.args();
        a.extend(["--ldproxy-cwd".into(), fixture.0.display().to_string()]);
        a
    }] {
        assert!(fixture
            .place(&mut args)
            .unwrap_err()
            .contains("exactly one --ldproxy-cwd"));
    }
}

#[test]
fn accepts_absolute_and_duplicate_search_paths_for_the_same_script() {
    let fixture = Fixture::new();
    fixture.script(ANCHOR);
    let mut args = fixture.args();
    args.extend(["-L".into(), fixture.0.join("ld").display().to_string()]);
    fixture.place(&mut args).unwrap();
    assert!(std::path::Path::new(&args[6]).is_absolute());
}

#[test]
fn reports_filesystem_errors_without_replacing_link_arguments() {
    let fixture = Fixture::new();
    fixture.script(ANCHOR);
    let mut args = fixture.args();
    let before = args.clone();
    fs::create_dir(fixture.0.join("out/sections-mp3-iram.ld")).unwrap();
    let error = fixture.place(&mut args).unwrap_err();
    assert!(error.contains("sections-mp3-iram.ld"));
    assert_eq!(args, before);
    assert_eq!(
        fs::read_to_string(fixture.0.join("ld/sections.ld")).unwrap(),
        ANCHOR
    );
}

#[test]
fn rejects_a_trailing_search_path_flag() {
    let fixture = Fixture::new();
    fixture.script(ANCHOR);
    let mut args = fixture.args();
    args.push("-L".into());
    assert!(fixture
        .place(&mut args)
        .unwrap_err()
        .contains("-L without a directory"));
}
