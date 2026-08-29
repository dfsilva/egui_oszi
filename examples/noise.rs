#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use rand::rngs::ThreadRng;
use rand_distr::Distribution;

use eframe::egui;

use egui_oszi::{PlotBand, TimeseriesLine, TimeseriesPlot, TimeseriesPlotMemory};

// ~1kHz of garbage
const SAMPLE_RATE: f64 = 1.0e3;
const POINTS_HISTORY: usize = 100_000;

struct NoiseExample {
    // our actual values
    sensor_data: VecDeque<(Instant, f32)>,
    // just for generating some fake sensor data
    rng: ThreadRng,
    first_frame: Instant,
    last_frame: Instant,
    // state for our plot widget
    plot_memory: TimeseriesPlotMemory<Instant, f32>,
}

impl NoiseExample {
    pub fn new() -> Self {
        Self {
            rng: rand::thread_rng(),
            sensor_data: VecDeque::with_capacity(POINTS_HISTORY + 2 * (SAMPLE_RATE as usize)), // some extra margin
            first_frame: Instant::now(),
            last_frame: Instant::now(),
            plot_memory: TimeseriesPlotMemory::new("mysensorplot"),
        }
    }

    /// Generate some "sensor" data with noise and occasional hick-ups
    fn update_sensor_data(&mut self) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let now = Instant::now();
        let elapsed = now - self.last_frame;
        let num_samples = (elapsed.as_secs_f64() * SAMPLE_RATE) as u32;
        let normal = rand_distr::Normal::new(0.0, 0.01).unwrap();
        let samples = (0..num_samples)
            .map(|i| {
                (
                    self.last_frame + i * Duration::from_secs_f64(1.0 / SAMPLE_RATE),
                    normal.sample(&mut self.rng),
                )
            })
            .map(|(x, sample)| {
                let t = (x - self.first_frame).as_secs_f32();
                let y = 1.4 + (t * 0.02).sin() * (t * 0.5).sin() * (t * 0.3).sin() + sample;

                // Simulate an occasional sensor issue where we get all-zero.
                // We might want to see this if it's important, so min/max downsampling should
                // not get rid of it.
                // ... Or we might want to just hide it so line go smooth.
                if sample < -0.035 {
                    (x, 0.0)
                } else {
                    (x, y)
                }
            })
            .filter(|(_x, y)| y.abs() < 2.0); // oh no, our sensor clipped and we missed some values

        self.sensor_data.extend(samples);
        if self.sensor_data.len() > POINTS_HISTORY {
            self.sensor_data
                .drain(0..(self.sensor_data.len() - POINTS_HISTORY));
        }

        self.last_frame = now;
    }
}

impl eframe::App for NoiseExample {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();
        #[cfg(feature = "profiling")]
        puffin::GlobalProfiler::lock().new_frame();
        #[cfg(feature = "profiling")]
        puffin_egui::profiler_window(ctx);

        self.update_sensor_data();

        egui::CentralPanel::default().show(ctx, |ui| {
            // Build a time series widget using plot memory.
            // Shade alternating five-second stretches, to show how a caller marks
            // regions of interest behind the traces.
            //
            // Band positions are in the plot's own coordinate space. With an `Instant`
            // axis that is seconds since the first sample, which only the memory knows
            // — hence `plot_x` rather than converting here.
            let elapsed = self.last_frame.duration_since(self.first_frame).as_secs_f64();
            let bands: Vec<PlotBand> = (0..(elapsed as u64 / 10))
                .filter_map(|i| {
                    let start = self.first_frame + Duration::from_secs(i * 10 + 5);
                    let end = start + Duration::from_secs(5);
                    Some(PlotBand::new(
                        "every other 5s",
                        self.plot_memory.plot_x(start)?,
                        self.plot_memory.plot_x(end)?,
                        egui::Color32::from_rgba_unmultiplied(0x45, 0x85, 0x88, 40),
                    ))
                })
                .collect();

            let timeseries = TimeseriesPlot::new(&mut self.plot_memory)
                .bands(bands)
                // Give an iterator over all values to be plotted
                .line(
                    TimeseriesLine::new("Ferrisses").unit("M🦀/s"),
                    self.sensor_data.iter().map(|(t, y)| (*t, *y)),
                );
            // That's it.
            ui.add(timeseries);
        });

        ctx.request_repaint_after(Duration::from_millis(1000) / 60);
    }
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    #[cfg(feature = "profiling")]
    puffin::set_scopes_on(true);

    let options = eframe::NativeOptions::default();
    let app = NoiseExample::new();
    eframe::run_native("NoiseExample", options, Box::new(|_cc| Ok(Box::new(app))))
}
