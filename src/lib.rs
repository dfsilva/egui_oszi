use egui::{Color32, Response, Ui, Vec2, Vec2b};
use egui_plot::{Legend, PlotPoints};

mod memory;
mod traits;

pub use memory::*;
pub use traits::*;

#[derive(Default)]
pub enum ViewMode {
    #[default]
    Complete,
    AttachedToEdge(f64), // TODO: use X axis diff unit?
}

pub struct TimeseriesLine {
    id: String,
    label: Option<String>,
    unit: Option<String>,
    color: Option<Color32>,
    width: Option<f32>,
}

impl TimeseriesLine {
    pub fn new(id: impl ToString) -> Self {
        let id = id.to_string();

        Self {
            id: id.clone(),
            label: Some(id), // TODO?
            unit: None,
            color: None,
            width: None,
        }
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn unit<S: ToString>(mut self, unit: S) -> Self {
        self.unit = Some(unit.to_string());
        self
    }
}

pub struct TimeseriesPlot<'mem, X, Y> {
    memory: &'mem mut TimeseriesPlotMemory<X, Y>,
    group: Option<&'mem mut TimeseriesGroup>,
    plot: egui_plot::Plot,
    lines: Vec<TimeseriesLine>,
    view_mode: ViewMode,
}

impl<
        'mem,
        X: TimeseriesXAxis,
        Y: Default + num_traits::Float + num_traits::float::TotalOrder + Into<f64>,
    > TimeseriesPlot<'mem, X, Y>
{
    pub fn new(memory: &'mem mut TimeseriesPlotMemory<X, Y>) -> Self {
        let id = memory.id;
        Self {
            memory,
            group: None,
            plot: egui_plot::Plot::new(id)
                .x_axis_position(egui_plot::VPlacement::Bottom)
                .y_axis_position(egui_plot::HPlacement::Right)
                .y_axis_width(3) // TODO
                .set_margin_fraction(Vec2::new(0.0, 0.05))
                .allow_scroll(false) // TODO: x only
                .allow_zoom([true, false])
                .allow_drag([true, false])
                .auto_bounds([false, true].into())
                .legend(Legend::default().position(egui_plot::Corner::LeftTop)),
            lines: Vec::new(),
            view_mode: ViewMode::default(),
        }
    }

    // TODO: either expose all relevant egui plot options here or maybe add a
    // away to access the raw Plot object

    pub fn width(mut self, width: f32) -> Self {
        self.plot = self.plot.height(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.plot = self.plot.height(height);
        self
    }

    pub fn legend(mut self, legend: Legend) -> Self {
        self.plot = self.plot.legend(legend);
        self
    }

    pub fn group(mut self, group: &'mem mut TimeseriesGroup) -> Self {
        self.group = Some(group);
        self
    }

    pub fn include_y(mut self, y: Y) -> Self {
        self.plot = self.plot.include_y(y);
        self
    }

    // TODO: change unit ot x axis diff
    pub fn follow_edge(mut self, duration: f64) -> Self {
        self.view_mode = ViewMode::AttachedToEdge(duration);
        self
    }

    pub fn line<
        'draw,
        I: Iterator<Item = (X, Y)> + ExactSizeIterator + DoubleEndedIterator + 'draw,
    >(
        mut self,
        line: TimeseriesLine,
        iterator: I,
    ) -> Self {
        self.memory
            .update_cache(&line.id, iterator.map(|(t, y)| (t, Some(y))));
        self.lines.push(line);
        self
    }

    //pub fn line_sparse<
    //    'b,
    //    Y: Into<f64>,
    //    I: TimeseriesIterator<X,Option<Y>> + Iterator<Item=(X,Option<Y>)> + ExactSizeIterator + DoubleEndedIterator + 'b
    //>(
    //    mut self,
    //    line: TimeseriesLine,
    //    iterator: I
    //) -> Self {
    //    self.memory.update_cache(line.id, iterator);
    //    self.lines.push(line);
    //    self
    //}
}

impl<
        'a,
        X: TimeseriesXAxis,
        Y: Default + num_traits::Float + num_traits::float::TotalOrder + Into<f64>,
    > egui::widgets::Widget for TimeseriesPlot<'a, X, Y>
{
    fn ui(mut self, ui: &mut Ui) -> Response {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        // Apply changes from other plots in the linked groupo
        if let Some(group) = &self.group {
            if let Some(width) = group.last_view_width {
                self.memory.last_view_width = width;
            }

            self.plot = self
                .plot
                .link_axis(group.link_group_name.clone(), true, group.link_y)
                .link_cursor(group.link_group_name.clone(), true, group.link_y);
        }

        if let ViewMode::AttachedToEdge(_duration) = self.view_mode {
            let end = self.memory.end().unwrap_or_default();
            self.plot = self
                .plot
                .include_x(end)
                .include_x(end - self.memory.last_view_width);
        }

        let plot_response = self
            .plot
            .legend(Legend::default().position(egui_plot::Corner::LeftTop))
            .show(ui, |plot_ui| {
                if self.memory.reset_auto_bounds_next_frame {
                    plot_ui.set_auto_bounds(Vec2b::new(true, plot_ui.auto_bounds().y));
                    self.memory.reset_auto_bounds_next_frame = false;
                }

                self.memory.last_auto_bounds = plot_ui.auto_bounds().x;

                for line in self.lines {
                    // TODO: cropping

                    let points = PlotPoints::new(self.memory.plot(&line.id, plot_ui.plot_bounds()));

                    let mut egui_line = egui_plot::Line::new(points);
                    if let Some(label) = line.label {
                        egui_line = egui_line.name(label);
                    }
                    if let Some(color) = line.color {
                        egui_line = egui_line.color(color);
                    }
                    if let Some(width) = line.width {
                        egui_line = egui_line.width(width);
                    }

                    plot_ui.line(egui_line);
                }

                //println!("{:?} {:?} {:?} {:?}",
                //         plot_ui.auto_bounds().x,
                //         self.memory.id,
                //         self.memory.reset_auto_bounds_next_frame,
                //         self.memory.last_view_width);
            });

        // For zooming, we have to reattach our plot to the edge afterwards
        if plot_response.response.hover_pos().is_some() && self.memory.last_auto_bounds {
            let zoom_delta = ui.input(|i| i.zoom_delta_2d());
            if zoom_delta.x != 1.0 {
                self.memory.reset_auto_bounds_next_frame = true;
                self.memory.last_view_width /= zoom_delta[0] as f64;
                if let Some(group) = self.group {
                    group.last_view_width = Some(self.memory.last_view_width);
                }
            }
        }

        plot_response.response
    }
}
