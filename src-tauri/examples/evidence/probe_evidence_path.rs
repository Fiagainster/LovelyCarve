//! Inspect a logical path through the same read-only API used by evidence plugins.
use lovelycarve_lib::core::{
    extensions::plugin::EvidenceRegistry, filesystem::evidence_session::EvidenceSession,
};
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        arguments.len() >= 3,
        "usage: probe_evidence_path <image> <root-id> <path>..."
    );
    let image = Arc::new(EvidenceSession::open(&arguments[0])?);
    let registry = EvidenceRegistry::default();
    let handle = registry.register("probe", image.id.clone(), image);
    if arguments
        .get(2)
        .is_some_and(|argument| argument == "--export-batch")
    {
        anyhow::ensure!(
            arguments.len() == 4,
            "--export-batch <JSON manifest of [source-path, output-path] pairs>"
        );
        let manifest = std::fs::read(&arguments[3])?;
        anyhow::ensure!(
            manifest.len() <= 4 * 1024 * 1024,
            "export manifest exceeds 4 MiB"
        );
        let requests: Vec<[String; 2]> = serde_json::from_slice(&manifest)?;
        anyhow::ensure!(requests.len() <= 4096, "export batch exceeds 4096 sources");
        let mut total = 0u64;
        for [source, destination] in requests {
            let node = registry
                .stat("probe", &handle.evidence_id, &arguments[1], &source)?
                .ok_or_else(|| anyhow::anyhow!("export source not found: {source}"))?;
            anyhow::ensure!(
                node.kind == "file" && !node.deleted && node.link_target.is_none(),
                "export source is not a regular existing file: {source}"
            );
            anyhow::ensure!(
                node.size <= 64 * 1024 * 1024,
                "export source exceeds 64 MiB: {source}"
            );
            total = total
                .checked_add(node.size)
                .ok_or_else(|| anyhow::anyhow!("export batch size overflow"))?;
            anyhow::ensure!(total <= 512 * 1024 * 1024, "export batch exceeds 512 MiB");
            let destination = std::path::Path::new(&destination);
            let parent = destination
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(std::path::Path::new("."));
            let mut output = tempfile::NamedTempFile::new_in(parent)?;
            let mut offset = 0;
            while offset < node.size {
                let bytes = registry.read(
                    "probe",
                    &handle.evidence_id,
                    &arguments[1],
                    &source,
                    offset,
                    usize::try_from((node.size - offset).min(4 * 1024 * 1024))?,
                )?;
                anyhow::ensure!(
                    !bytes.is_empty() && bytes.len() as u64 <= node.size - offset,
                    "export short / oversized read at {source}:{offset}"
                );
                std::io::Write::write_all(&mut output, &bytes)?;
                offset += bytes.len() as u64;
            }
            anyhow::ensure!(offset == node.size, "export size mismatch: {source}");
            output.as_file().sync_all()?;
            output.persist_noclobber(destination)?;
        }
        return Ok(());
    }
    if arguments
        .get(2)
        .is_some_and(|argument| argument == "--export" || argument == "--export-range")
    {
        let ranged = arguments[2] == "--export-range";
        anyhow::ensure!(arguments.len() == if ranged { 7 } else { 5 },
            "--export <evidence-path> <output-path> or --export-range <evidence-path> <offset> <length> <output-path>");
        let node = registry
            .stat("probe", &handle.evidence_id, &arguments[1], &arguments[3])?
            .ok_or_else(|| anyhow::anyhow!("export source not found"))?;
        let (start, total, destination) = if ranged {
            let start = arguments[4].parse::<u64>()?;
            let length = arguments[5].parse::<u64>()?;
            anyhow::ensure!(length <= 64 * 1024 * 1024, "range exceeds 64 MiB");
            anyhow::ensure!(
                start
                    .checked_add(length)
                    .is_some_and(|end| end <= node.size),
                "range outside source"
            );
            (start, length, std::path::Path::new(&arguments[6]))
        } else {
            (0, node.size, std::path::Path::new(&arguments[4]))
        };
        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        let mut offset = 0;
        while offset < total {
            let bytes = registry.read(
                "probe",
                &handle.evidence_id,
                &arguments[1],
                &arguments[3],
                start + offset,
                usize::try_from((total - offset).min(4 * 1024 * 1024))?,
            )?;
            anyhow::ensure!(!bytes.is_empty(), "short read at {offset}");
            std::io::Write::write_all(&mut output, &bytes)?;
            offset += bytes.len() as u64;
        }
        anyhow::ensure!(offset == total, "export size mismatch");
        output.as_file().sync_all()?;
        output.persist_noclobber(destination)?;
        return Ok(());
    }
    for path in &arguments[2..] {
        // `read:<path>` dumps up to 4 MiB of file content for ground-truth
        // checks; plain paths keep the stat (+ listing) behavior.
        if let Some(file) = path.strip_prefix("read:") {
            let bytes = registry.read(
                "probe",
                &handle.evidence_id,
                &arguments[1],
                file,
                0,
                4 * 1024 * 1024,
            )?;
            print!("{file}: ");
            match String::from_utf8(bytes) {
                Ok(text) => println!("{text}"),
                Err(error) => println!("{:?}", error.as_bytes()),
            }
            continue;
        }
        let node = registry.stat("probe", &handle.evidence_id, &arguments[1], path)?;
        println!("{path}: {}", serde_json::to_string(&node)?);
        if node.is_some_and(|node| node.kind == "dir") {
            let children = registry.list("probe", &handle.evidence_id, &arguments[1], path)?;
            println!("{}", serde_json::to_string(&children)?);
        }
    }
    Ok(())
}
