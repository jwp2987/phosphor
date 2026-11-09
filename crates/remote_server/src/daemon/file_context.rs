//! Batch `ReadFileContext` (Wave H parity, family a).
//!
//! Gathers each file's bytes from this host's filesystem and hands them to
//! [`crate::shellcmd::read::assemble`], the same function the client's shell backend uses, so a
//! file reads identically whichever arm serves it (caps, line ranges, the truncation marker
//! `apply_diff` relies on, binary detection, `failed_files` wording).

use std::io::Read;

use super::jobs::Cancel;
use crate::naming::expand_tilde;
use crate::proto::{ReadFileContextRequest, ReadFileContextResponse};
use crate::shellcmd::read::{self, FileProbe, GatherPlan, LoadedFile};

/// Answers a whole request. Runs on a worker thread; stops early (and the reply is dropped) when
/// `cancel` fires.
pub(super) fn read_file_context(
    request: &ReadFileContextRequest,
    cancel: &Cancel,
) -> ReadFileContextResponse {
    log::info!("Handling ReadFileContext ({} files)", request.files.len());
    let mut probes = Vec::with_capacity(request.files.len());
    for file in &request.files {
        if cancel.is_cancelled() {
            break;
        }
        probes.push(gather(&file.path, &read::plan(file, request)));
    }
    probes.resize(request.files.len(), FileProbe::NotFound);
    read::assemble(request, probes)
}

fn gather(path: &str, plan: &GatherPlan) -> FileProbe {
    let path = expand_tilde(path);
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return FileProbe::NotFound,
        Err(e) => return FileProbe::Failed(e.to_string()),
    };
    if metadata.is_dir() {
        return FileProbe::Failed("Is a directory (os error 21)".to_string());
    }
    let mut file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(e) => return FileProbe::Failed(e.to_string()),
    };
    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);

    let mut bytes: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    let mut newlines = 0usize;
    let mut reached_eof = false;
    while bytes.len() < plan.prefix_bytes {
        let want = chunk.len().min(plan.prefix_bytes - bytes.len());
        let n = match file.read(&mut chunk[..want]) {
            Ok(0) => {
                reached_eof = true;
                break;
            }
            Ok(n) => n,
            Err(e) => return FileProbe::Failed(e.to_string()),
        };
        let read = &chunk[..n];
        if let Some(through) = plan.through_line {
            // Stop right after the newline that ends line `through`.
            let mut cut = None;
            for (i, _) in read.iter().enumerate().filter(|(_, b)| **b == b'\n') {
                newlines += 1;
                if newlines == through {
                    cut = Some(i + 1);
                    break;
                }
            }
            if let Some(cut) = cut {
                bytes.extend_from_slice(&read[..cut]);
                break;
            }
        }
        bytes.extend_from_slice(read);
    }
    // Count the lines of the whole file when it was not read to the end.
    let size = metadata.len();
    let complete = reached_eof || (plan.through_line.is_none() && size <= bytes.len() as u64);
    let mut total_lines = None;
    if !complete {
        let mut lines = bytes.iter().filter(|b| **b == b'\n').count();
        let mut last = bytes.last().copied();
        loop {
            match file.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    lines += chunk[..n].iter().filter(|b| **b == b'\n').count();
                    last = Some(chunk[n - 1]);
                }
                Err(e) => return FileProbe::Failed(e.to_string()),
            }
        }
        // A last line without a newline is still a line.
        if last.is_some_and(|b| b != b'\n') {
            lines += 1;
        }
        total_lines = Some(lines);
    }
    // A scan that stopped after `through` lines but found the file ends there is complete.
    let complete = match (plan.through_line, total_lines) {
        (Some(through), Some(total)) => total <= through,
        _ => complete,
    };
    FileProbe::Loaded(LoadedFile {
        size,
        mtime_ms,
        bytes,
        complete,
        total_lines: if complete { None } else { total_lines },
    })
}

#[cfg(test)]
#[path = "file_context_tests.rs"]
mod tests;
