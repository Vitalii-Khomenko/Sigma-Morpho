use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct TorController {
    control_address: String,
    password: Option<String>,
}

impl TorController {
    pub fn new(control_address: String, password: Option<String>) -> Self {
        Self {
            control_address,
            password,
        }
    }

    /// Отправляет сигнал NEWNYM для смены IP-адреса
    pub async fn request_new_ip(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = tokio::time::timeout(
            Duration::from_secs(5),
            TcpStream::connect(&self.control_address)
        ).await??;

        // 1. Аутентификация
        let auth_cmd = match &self.password {
            Some(pwd) => format!("AUTHENTICATE \"{}\"\r\n", pwd),
            None => "AUTHENTICATE \"\"\r\n".to_string(),
        };
        
        stream.write_all(auth_cmd.as_bytes()).await?;
        self.read_and_check(&mut stream, "250").await?;

        // 2. Отправка сигнала смены цепочки
        stream.write_all(b"SIGNAL NEWNYM\r\n").await?;
        self.read_and_check(&mut stream, "250").await?;

        Ok(())
    }

    /// Вспомогательная функция для чтения ответа от Control Port
    async fn read_and_check(&self, stream: &mut TcpStream, expected_code: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut buffer = [0; 128];
        let n = stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        
        if response.starts_with(expected_code) {
            Ok(())
        } else {
            Err(format!("Tor Error: Expected {}, got: {}", expected_code, response).into())
        }
    }
}
