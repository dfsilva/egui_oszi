use egui::{Color32, Response, Shape, Ui, Vec2, Vec2b};
use egui_plot::{
    Legend, PlotBounds, PlotGeometry, PlotItem, PlotItemBase, PlotPoints, PlotPoint,
    PlotTransform,
};

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


/// A shaded vertical band spanning the full height of the plot.
///
/// Used to mark stretches of the x-axis that belong together — a flight regime, a
/// dropout, a paused section — so a reader can see which part of a trace they are
/// looking at without cross-referencing a separate table.
///
/// Contributes nothing to the plot's auto-bounds. A band drawn to the current
/// y-bounds would be folded back into the next frame's auto-bounds along with the
/// margin `egui_plot` adds, and the view would creep wider every frame. Bands always
/// mark stretches of a timeline the traces already cover, so they never need to extend
/// the x-range either.
pub struct PlotBand {
    base: PlotItemBase,
    start: f64,
    end: f64,
    color: Color32,
}

impl PlotBand {
    pub fn new(name: impl ToString, start: f64, end: f64, color: Color32) -> Self {
        // Accept the range either way round so callers need not normalise it.
        let (start, end) = if start <= end { (start, end) } else { (end, start) };
        Self {
            base: PlotItemBase::new(name.to_string()),
            start,
            end,
            color,
        }
    }
}

impl PlotItem for PlotBand {
    fn shapes(&self, _ui: &Ui, transform: &PlotTransform, shapes: &mut Vec<Shape>) {
        // Fill the full visible height, exactly as `VLine` draws its full-height line.
        let bounds = transform.bounds();
        let top_left =
            transform.position_from_point(&PlotPoint::new(self.start, bounds.max()[1]));
        let bottom_right =
            transform.position_from_point(&PlotPoint::new(self.end, bounds.min()[1]));
        let rect = egui::Rect::from_two_pos(top_left, bottom_right);
        // A band narrower than a pixel would vanish; widen it so a brief regime is
        // still visible rather than silently absent.
        let rect = if rect.width() < 1.0 {
            egui::Rect::from_min_size(rect.min, egui::vec2(1.0, rect.height()))
        } else {
            rect
        };
        shapes.push(Shape::rect_filled(rect, 0.0, self.color));
    }

    fn initialize(&mut self, _x_range: std::ops::RangeInclusive<f64>) {}

    fn color(&self) -> Color32 {
        self.color
    }

    fn geometry(&self) -> PlotGeometry<'_> {
        PlotGeometry::None
    }

    fn bounds(&self) -> PlotBounds {
        PlotBounds::NOTHING
    }

    fn base(&self) -> &PlotItemBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut PlotItemBase {
        &mut self.base
    }
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
    plot: egui_plot::Plot<'mem>,
    lines: Vec<TimeseriesLine>,
    bands: Vec<PlotBand>,
    zoom_to_band_on_click: bool,
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
                .y_axis_min_width(3.0) // TODO
                .set_margin_fraction(Vec2::new(0.0, 0.05))
                .allow_scroll(false)
                .allow_zoom(true)
                .allow_boxed_zoom(true)
                .allow_drag(true)
                .auto_bounds(Vec2b::new(true, true))
                .legend(Legend::default().position(egui_plot::Corner::LeftTop)),
            lines: Vec::new(),
            bands: Vec::new(),
            zoom_to_band_on_click: false,
            view_mode: ViewMode::default(),
        }
    }

    pub fn allow_drag(mut self, drag: impl Into<Vec2b>) -> Self {
        self.plot = self.plot.allow_drag(drag);
        self
    }

    pub fn allow_zoom(mut self, zoom: impl Into<Vec2b>) -> Self {
        self.plot = self.plot.allow_zoom(zoom);
        self
    }

    pub fn allow_scroll(mut self, scroll: impl Into<Vec2b>) -> Self {
        self.plot = self.plot.allow_scroll(scroll);
        self
    }

    pub fn allow_boxed_zoom(mut self, boxed_zoom: bool) -> Self {
        self.plot = self.plot.allow_boxed_zoom(boxed_zoom);
        self
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

    /// Shade stretches of the x-axis behind the traces.
    ///
    /// Bands are drawn before the lines so they read as background rather than as
    /// data, and they are excluded from the legend — with one band per span a legend
    /// entry each would bury the actual series.
    pub fn bands(mut self, bands: impl IntoIterator<Item = PlotBand>) -> Self {
        self.bands.extend(bands);
        self
    }

    /// Clicking inside a band zooms the x-axis to it.
    ///
    /// Handled here rather than by the caller because the bands' extents are already
    /// known here, and because the resulting bounds have to be applied on the *next*
    /// frame — the click is only known once the plot has been drawn.
    ///
    /// `egui_plot` already resets to the full view on double-click, so the pair reads
    /// as zoom-in / zoom-out without needing another control.
    pub fn zoom_to_band_on_click(mut self, enabled: bool) -> Self {
        self.zoom_to_band_on_click = enabled;
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
                .link_axis(
                    group.link_group_name.clone(),
                    Vec2b::new(true, group.link_y),
                )
                .link_cursor(
                    group.link_group_name.clone(),
                    Vec2b::new(true, group.link_y),
                );
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

                // A zoom requested by last frame's click. Applied before anything is
                // drawn so the traces are already cropped to it this frame.
                if let Some((start, end)) = self.memory.pending_x_bounds.take() {
                    plot_ui.set_plot_bounds_x(start..=end);
                }

                if self.zoom_to_band_on_click {
                    // A plain click is otherwise unused: dragging pans, scrolling and
                    // pinching zoom, and boxed zoom is on the right button.
                    if plot_ui.response().clicked() {
                        if let Some(at) = plot_ui.pointer_coordinate() {
                            if let Some(band) = self
                                .bands
                                .iter()
                                .find(|b| at.x >= b.start && at.x <= b.end)
                            {
                                self.memory.pending_x_bounds = Some((band.start, band.end));
                            }
                        }
                    }
                }

                // Behind the traces, so they read as background.
                for band in self.bands {
                    plot_ui.add(band);
                }

                for line in self.lines {
                    // TODO: cropping

                    let points = PlotPoints::new(self.memory.plot(&line.id, plot_ui.plot_bounds()));

                    // egui_plot 0.34 requires name as first arg: Line::new(name, points)
                    let line_name = line.label.unwrap_or_else(|| line.id.clone());
                    let mut egui_line = egui_plot::Line::new(line_name, points);
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

        // Track view width for AttachedToEdge mode
        if let Some(group) = self.group {
            if plot_response.response.hover_pos().is_some() {
                let zoom_delta = ui.input(|i| i.zoom_delta_2d());
                if zoom_delta.x != 1.0 {
                    self.memory.last_view_width /= zoom_delta[0] as f64;
                    group.last_view_width = Some(self.memory.last_view_width);
                }
            }
        }

        plot_response.response
    }
}

#[cfg(test)]
mod band_tests {
    use super::*;

    #[test]
    fn a_reversed_range_is_normalised() {
        // Callers build these from span boundaries; accepting either order means a
        // caller that subtracts the wrong way round still gets a visible band rather
        // than a silently empty one.
        let band = PlotBand::new("x", 9.0, 4.0, Color32::RED);
        assert_eq!((band.start, band.end), (4.0, 9.0));
    }

    #[test]
    fn an_ordered_range_is_left_alone() {
        let band = PlotBand::new("x", 4.0, 9.0, Color32::RED);
        assert_eq!((band.start, band.end), (4.0, 9.0));
    }

    // Placing a band on a non-f64 axis needs the plot's own origin, and that origin is
    // the first sample added. Answering before any data exists would have to invent one,
    // which would then shift every later sample relative to it.
    #[test]
    fn plot_x_declines_to_answer_before_there_is_data() {
        let memory: TimeseriesPlotMemory<std::time::Instant, f32> =
            TimeseriesPlotMemory::new("empty");
        assert_eq!(memory.plot_x(std::time::Instant::now()), None);
    }

    // With data present it converts against that same origin, so a band lands where the
    // caller means it to.
    #[test]
    fn plot_x_measures_from_the_first_sample() {
        use std::time::{Duration, Instant};
        let mut memory: TimeseriesPlotMemory<Instant, f32> = TimeseriesPlotMemory::new("t");
        let origin = Instant::now();
        memory.update_cache(
            &"line".to_string(),
            vec![(origin, Some(0.0f32)), (origin + Duration::from_secs(1), Some(1.0f32))]
                .into_iter(),
        );
        let five = memory
            .plot_x(origin + Duration::from_secs(5))
            .expect("origin is established once data exists");
        assert!((five - 5.0).abs() < 1e-6, "got {five}");
        assert_eq!(memory.plot_x(origin), Some(0.0));
    }

    // The click lands on whichever band contains it. Overlaps should not happen — a
    // caller derives bands from spans that tile a timeline — but if one slips through,
    // taking the first keeps the behaviour predictable rather than order-dependent on
    // whatever the layout happened to be.
    #[test]
    fn a_click_selects_the_band_it_lands_in() {
        let bands = [
            PlotBand::new("a", 0.0, 10.0, Color32::RED),
            PlotBand::new("b", 10.0, 20.0, Color32::BLUE),
        ];
        let hit = |x: f64| {
            bands
                .iter()
                .find(|b| x >= b.start && x <= b.end)
                .map(|b| (b.start, b.end))
        };
        assert_eq!(hit(5.0), Some((0.0, 10.0)));
        assert_eq!(hit(15.0), Some((10.0, 20.0)));
        // Outside every band: nothing to zoom to, so the click does nothing.
        assert_eq!(hit(25.0), None);
        assert_eq!(hit(-1.0), None);
    }

    // Bands mark stretches of a timeline the traces already cover. Contributing bounds
    // would fold the band's extent — and egui_plot's auto-bounds margin — back into the
    // next frame's view, which creeps wider every frame.
    #[test]
    fn a_band_contributes_no_bounds() {
        let band = PlotBand::new("x", 4.0, 9.0, Color32::RED);
        let bounds = PlotItem::bounds(&band);
        assert_eq!(bounds, PlotBounds::NOTHING);
    }
}
