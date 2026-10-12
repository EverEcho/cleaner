use std::process::{Command, Output};
pub trait QueryCommand {
    fn query_output(&mut self) -> std::io::Result<Output>;
}

impl QueryCommand for Command {
    fn query_output(&mut self) -> std::io::Result<Output> {
        use std::{
            io::Read,
            process::Stdio,
            thread,
            time::{Duration, Instant},
        };
        let mut child = self
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let read_out = thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).map(|_| bytes)
        });
        let read_err = thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).map(|_| bytes)
        });
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if start.elapsed() > Duration::from_secs(20) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "软件信息查询超时",
                ));
            }
            thread::sleep(Duration::from_millis(50));
        };
        let stdout = read_out
            .join()
            .map_err(|_| std::io::Error::other("读取输出失败"))??;
        let stderr = read_err
            .join()
            .map_err(|_| std::io::Error::other("读取错误输出失败"))??;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}
