/// 写入调试日志到文件（打包后可用）
pub fn debug_log(msg: &str) {
    use std::io::Write;
    let log_path = std::env::temp_dir().join("meowmic-debug.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        let elapsed = std::time::Instant::now().elapsed();
        let now = chrono::Local::now().format("%H:%M:%S%.3f");
        let _ = writeln!(f, "[{}] {:?} | {}", now, elapsed, msg);
    }
    // 日志轮转：超过 2000 行时只保留最近 1000 行
    trim_log_file(&log_path, 2000, 1000);
}

/// 日志文件轮转：当行数超过 max_lines 时，只保留最后 keep_lines 行
fn trim_log_file(path: &std::path::Path, max_lines: usize, keep_lines: usize) {
    use std::io::{BufRead, BufReader, SeekFrom, Seek, Write};
    let file = match std::fs::OpenOptions::new().read(true).write(true).open(path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let reader = BufReader::new(&file);
    let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
    if lines.len() <= max_lines {
        return;
    }
    let keep: Vec<&str> = lines.iter().rev().take(keep_lines).rev().map(|s| s.as_str()).collect();
    let mut file = file;
    file.seek(SeekFrom::Start(0)).ok();
    file.set_len(0).ok();
    for line in keep {
        let _ = writeln!(file, "{}", line);
    }
}
