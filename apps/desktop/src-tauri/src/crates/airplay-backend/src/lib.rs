use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bridge_core::{LatencyProfile, Receiver};

#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub profile: LatencyProfile,
    pub volume: f32,
}

#[async_trait]
pub trait AirPlayStream: Send {
    async fn send_pcm(&mut self, samples: Vec<i16>, sample_rate: u32, channels: u8) -> Result<()>;
    async fn set_volume(&mut self, volume: f32) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
}

#[async_trait]
pub trait AirPlayBackend: Send + Sync {
    async fn discover(&self) -> Result<Vec<Receiver>>;
    async fn open(&self, receiver: &Receiver, config: StreamConfig) -> Result<Box<dyn AirPlayStream>>;
}

#[cfg(feature = "airsend")]
#[derive(Debug, Default, Clone, Copy)]
pub struct AirSendBackend;

#[cfg(feature = "airsend")]
#[async_trait]
impl AirPlayBackend for AirSendBackend {
    async fn discover(&self) -> Result<Vec<Receiver>> {
        use cap_core::DeviceKind;
        use std::{collections::HashSet, time::Duration};

        let devices = cap_core::browse_once(Duration::from_millis(1800))
            .await
            .map_err(|e| anyhow!("AirPlay discovery: {e}"))?;
        let mut seen = HashSet::new();
        let mut receivers = Vec::new();

        for device in devices {
            if device.kind != DeviceKind::HomePod || !device.supports_airplay2 {
                continue;
            }
            let Some(ip) = device
                .addresses
                .iter()
                .find(|ip| ip.is_ipv4())
                .or_else(|| device.addresses.first())
                .copied()
            else {
                continue;
            };
            let dedup_key = format!("{ip}:{}", device.port);
            if !seen.insert(dedup_key) {
                continue;
            }
            receivers.push(Receiver {
                id: device.id,
                name: device.name,
                address: ip.to_string(),
                port: device.port,
                model: device.model,
                is_stereo_pair: false,
            });
        }
        receivers.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(receivers)
    }

    async fn open(&self, receiver: &Receiver, config: StreamConfig) -> Result<Box<dyn AirPlayStream>> {
        let ip = receiver
            .address
            .parse()
            .map_err(|e| anyhow!("invalid receiver address {}: {e}", receiver.address))?;
        let descriptor = cap_core::pairing::DeviceDescriptor {
            ip,
            port: receiver.port,
            name: receiver.name.clone(),
            mac: None,
            model: receiver.model.clone(),
            features: None,
        };

        // IMPORTANT: unlike the old pinned AirSend core, the newer core exposes
        // receiver-side latency negotiation. Make the UI profile real:
        // Ultra -> Gaming (~250 ms - 1 s)
        // Low   -> Video  (~350 ms - 2 s)
        // Stable-> Music  (~500 ms - 3 s)
        let receiver_latency = match config.profile {
            LatencyProfile::Ultra => cap_core::LatencyProfile::Gaming,
            LatencyProfile::Low => cap_core::LatencyProfile::Video,
            LatencyProfile::Stable => cap_core::LatencyProfile::Music,
        };

        let handle = cap_core::streaming::open_live_stream(
            descriptor,
            Some(config.volume.clamp(0.0, 1.0)),
            Some(receiver_latency),
        )
        .await
        .map_err(|e| anyhow!("HomePod stream open failed: {e}"))?;

        Ok(Box::new(AirSendStream { handle: Some(handle) }))
    }
}

#[cfg(feature = "airsend")]
struct AirSendStream {
    handle: Option<cap_core::streaming::StreamHandle>,
}

#[cfg(feature = "airsend")]
#[async_trait]
impl AirPlayStream for AirSendStream {
    async fn send_pcm(&mut self, samples: Vec<i16>, sample_rate: u32, channels: u8) -> Result<()> {
        let handle = self.handle.as_ref().ok_or_else(|| anyhow!("stream already closed"))?;
        if sample_rate != handle.sample_rate() || channels != handle.channels() {
            return Err(anyhow!(
                "capture format {sample_rate}Hz/{channels}ch does not match AirPlay {}Hz/{}ch",
                handle.sample_rate(),
                handle.channels()
            ));
        }
        if !handle.push_pcm(samples) {
            return Err(anyhow!("AirPlay PCM queue full"));
        }
        Ok(())
    }

    async fn set_volume(&mut self, volume: f32) -> Result<()> {
        self.handle
            .as_ref()
            .ok_or_else(|| anyhow!("stream already closed"))?
            .set_volume(volume)
            .await
            .map_err(|e| anyhow!("set HomePod volume: {e}"))
    }

    async fn close(&mut self) -> Result<()> {
        self.handle.take();
        Ok(())
    }
}

#[derive(Default)]
pub struct MockBackend {
    pub receivers: Vec<Receiver>,
}

#[async_trait]
impl AirPlayBackend for MockBackend {
    async fn discover(&self) -> Result<Vec<Receiver>> {
        Ok(self.receivers.clone())
    }

    async fn open(&self, _receiver: &Receiver, config: StreamConfig) -> Result<Box<dyn AirPlayStream>> {
        Ok(Box::new(MockStream { volume: config.volume }))
    }
}

struct MockStream {
    volume: f32,
}

#[async_trait]
impl AirPlayStream for MockStream {
    async fn send_pcm(&mut self, _samples: Vec<i16>, _sample_rate: u32, _channels: u8) -> Result<()> {
        Ok(())
    }

    async fn set_volume(&mut self, volume: f32) -> Result<()> {
        self.volume = volume.clamp(0.0, 1.0);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}
