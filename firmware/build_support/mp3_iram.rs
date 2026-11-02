// ABOUTME: Places the MP3 C instructions and literals in the generated IRAM section.
// ABOUTME: Keeps the ESP-IDF linker script intact and derives a Cargo-local copy.
use std::{
    fs,
    path::{Path, PathBuf},
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

pub fn place_mp3(args: &mut [String], out_dir: &Path) -> Result<PathBuf, String> {
    let scripts: Vec<_> = args
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| pair[0] == "-T" && pair[1] == "sections.ld")
        .map(|(i, _)| i + 1)
        .collect();
    if scripts.len() != 1 {
        return Err("MP3 IRAM placement requires exactly one -T sections.ld argument".into());
    }
    let cwd_flags: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| *arg == "--ldproxy-cwd")
        .map(|(i, _)| i)
        .collect();
    if cwd_flags.len() != 1 || args.get(cwd_flags[0] + 1).is_none() {
        return Err("MP3 IRAM placement requires exactly one --ldproxy-cwd directory".into());
    }
    let cwd = Path::new(&args[cwd_flags[0] + 1]);
    let mut candidates = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let directory = if args[index] == "-L" {
            index += 1;
            Some(
                args.get(index)
                    .ok_or("MP3 IRAM placement found -L without a directory")?
                    .as_str(),
            )
        } else {
            args[index].strip_prefix("-L")
        };
        if let Some(directory) = directory {
            let script = cwd.join(directory).join("sections.ld");
            if script.is_file() {
                let script = script
                    .canonicalize()
                    .map_err(|error| format!("{}: {error}", script.display()))?;
                if !candidates.contains(&script) {
                    candidates.push(script);
                }
            }
        }
        index += 1;
    }
    if candidates.len() != 1 {
        return Err(format!(
            "MP3 IRAM placement requires exactly one sections.ld in linker search paths; found {}",
            candidates.len()
        ));
    }
    let original = &candidates[0];
    let script =
        fs::read_to_string(original).map_err(|error| format!("{}: {error}", original.display()))?;
    if script.matches(ANCHOR).count() != 1 {
        return Err(format!(
            "MP3 IRAM placement requires exactly one IRAM anchor in {}",
            original.display()
        ));
    }
    let selectors: String = MEMBERS
        .iter()
        .map(|member| {
            format!(
                "    *libesp_shazam_mp3-*.rlib:*-{member}.o(.literal .literal.* .text .text.*)\n"
            )
        })
        .collect();
    let output = out_dir
        .canonicalize()
        .map_err(|error| format!("{}: {error}", out_dir.display()))?
        .join("sections-mp3-iram.ld");
    let output_arg = output
        .to_str()
        .ok_or("MP3 IRAM linker-script path is not UTF-8")?;
    fs::write(
        &output,
        script.replacen(ANCHOR, &format!("{ANCHOR}{selectors}"), 1),
    )
    .map_err(|error| format!("{}: {error}", output.display()))?;
    args[scripts[0]] = output_arg.to_owned();
    Ok(original.clone())
}
