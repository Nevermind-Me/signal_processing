use crate::virtual_audio_sink::*;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream};
use ringbuf::HeapRb;
use ringbuf::traits::*;
use std::eprintln;

pub struct AudioHandling {
    audio_cable: VirtualAudioCable,
    input_device: Device,
    output_device: Device,
    config: cpal::StreamConfig,
    pub num_channels: usize,
    pub sample_rate: f32,
    _input_stream: Option<Stream>,
    _output_stream: Option<Stream>,
}

impl AudioHandling {
    pub fn init_audio_handling() -> Result<Self, Box<dyn std::error::Error>> {
        let audio_cable = VirtualAudioCable::create()?;

        let host = cpal::default_host();
        let input_device = host.default_input_device().expect("No input device");
        let output_device = host.default_output_device().expect("No output device");

        let config: cpal::StreamConfig = input_device.default_input_config()?.into();
        let num_channels = config.channels as usize;
        let sample_rate = config.sample_rate as f32;

        Ok(Self {
            audio_cable,
            input_device,
            output_device,
            config,
            num_channels,
            sample_rate,
            _input_stream: None,
            _output_stream: None,
        })
    }

    pub fn start_audio_streams<F>(&mut self, mut func: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(f32, usize) -> f32 + Send + 'static,
    {
        let mut channel_idx = 0;
        let num_channels = self.num_channels;

        // Ring buffer setup (~100ms latency)
        let buffer_size = ((self.sample_rate * 0.1) as usize) * num_channels;
        let ring_buffer = HeapRb::<f32>::new(buffer_size);
        let (mut producer, mut consumer) = ring_buffer.split();

        // Input callback
        let input_stream = self.input_device.build_input_stream(
            self.config.clone(),
            move |data: &[f32], _| {
                for &sample in data {
                    let _ = producer.try_push(sample);
                }
            },
            |err| eprintln!("Input stream error: {err}"),
            None,
        )?;

        // Output callback
        let output_stream = self.output_device.build_output_stream(
            self.config,
            move |data: &mut [f32], _| {
                for sample in data.iter_mut() {
                    let raw_sample = consumer.try_pop().unwrap_or(0.0);

                    *sample = func(raw_sample, channel_idx);

                    channel_idx = (channel_idx + 1) % num_channels;
                }
            },
            |err| eprintln!("Output stream error: {err}"),
            None,
        )?;

        input_stream.play()?;
        output_stream.play()?;
        self.audio_cable.route_app_streams();

        // Store the streams inside self so they stay alive
        self._input_stream = Some(input_stream);
        self._output_stream = Some(output_stream);

        Ok(())
    }
}
