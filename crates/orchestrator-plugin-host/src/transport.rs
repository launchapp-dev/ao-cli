use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

pub struct StdioTransport<R, W> {
    reader: BufReader<R>,
    writer: W,
}

impl<R, W> StdioTransport<R, W>
where
    R: tokio::io::AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader: BufReader::new(reader), writer }
    }

    pub async fn read_message<T>(&mut self) -> Result<Option<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut line = String::new();
        let read = self.reader.read_line(&mut line).await?;
        if read == 0 {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(line.trim())?))
    }

    pub async fn write_message<T>(&mut self, message: &T) -> Result<()>
    where
        T: Serialize,
    {
        let mut line = serde_json::to_string(message)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }
}
