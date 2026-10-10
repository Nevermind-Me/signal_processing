use std::format;
use std::process::Command;

pub struct VirtualAudioCable {
    original_sink: String,
    module_id: String,
}

impl VirtualAudioCable {
    pub fn create() -> Result<Self, Box<dyn std::error::Error>> {
        // Save original physical headphones/speakers sink name
        let output = Command::new("pactl").arg("get-default-sink").output()?;
        let original_sink = String::from_utf8(output.stdout)?.trim().to_string();

        // Create the virtual sink
        let output = Command::new("pactl")
            .args([
                "load-module",
                "module-null-sink",
                "sink_name=RustAudioSink",
                "sink_properties=device.description=\"Rust_Audio_Router\"",
            ])
            .output()?;
        let module_id = String::from_utf8(output.stdout)?.trim().to_string();

        // Set system default sink to RustAudioSink
        Command::new("pactl")
            .args(["set-default-sink", "RustAudioSink"])
            .status()?;

        Command::new("pactl")
            .args(["set-default-source", "RustAudioSink.monitor"])
            .status()?;

        Ok(Self {
            original_sink,
            module_id,
        })
    }

    /// Moves this process's streams: Playback -> Headphones, Recording -> RustAudioSink.monitor
    pub fn route_app_streams(&self) {
        let pid = std::process::id().to_string();

        // Give PipeWire/PulseAudio 150ms to register the newly opened CPAL streams
        std::thread::sleep(std::time::Duration::from_millis(150));

        // Move Rust App Playback stream to Physical Headphones
        if let Ok(output) = Command::new("pactl").args(["list", "sink-inputs"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(id) = find_stream_id_for_pid(&text, "Sink Input #", &pid) {
                let _ = Command::new("pactl")
                    .args(["move-sink-input", &id, &self.original_sink])
                    .status();
            }
        }

        // Move Rust App Recording stream to RustAudioSink.monitor
        if let Ok(output) = Command::new("pactl")
            .args(["list", "source-outputs"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(id) = find_stream_id_for_pid(&text, "Source Output #", &pid) {
                let _ = Command::new("pactl")
                    .args(["move-source-output", &id, "RustAudioSink.monitor"])
                    .status();
            }
        }
    }
}

// Restores original audio routing when the application exits
impl Drop for VirtualAudioCable {
    fn drop(&mut self) {
        let _ = Command::new("pactl")
            .args(["set-default-sink", &self.original_sink])
            .status();
        let _ = Command::new("pactl")
            .args(["unload-module", &self.module_id])
            .status();
    }
}

/// Helper to parse stream IDs matching current process ID from pactl output
fn find_stream_id_for_pid(output: &str, header_prefix: &str, pid: &str) -> Option<String> {
    let search_pid = format!("application.process.id = \"{pid}\"");
    for block in output.split(header_prefix) {
        if block.contains(&search_pid) {
            let first_line = block.lines().next()?.trim();
            let clean_id = first_line.split_whitespace().next()?;
            return Some(clean_id.to_string());
        }
    }
    None
}
