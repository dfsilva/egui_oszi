use std::collections::HashMap;

use egui_plot::PlotBounds;

use crate::traits::*;

// min/max downsampling produces two values per bucket, so this means
// a downsampling factor of 4.
const DOWNSAMPLING_BUCKET_SIZE: usize = 8;

const MAX_POINTS: usize = 4000; // TODO: use screen size

const MAX_DOWNSAMPLING_STEPS: usize = 5;

#[derive(Debug)]
struct CacheDescriptor<X, Y> {
    len: usize,
    first_data_point: Option<(X, Option<Y>)>,
    //last_data_point: Option<(X, Option<Y>)>,
}

#[derive(Clone, Copy, Debug)]
pub enum DownsamplingMethod {
    None,
    MinMax,
    Mean,
}

impl DownsamplingMethod {
    fn downsample<Y: Default + num_traits::Float + num_traits::float::TotalOrder>(
        &self,
        bucket: &[(f64, Y)],
    ) -> [(f64, Y); 2] {
        match self {
            Self::None => {
                [(0.0, Y::default()), (0.0, Y::default())] // TODO
            }
            Self::MinMax => {
                let (min_i, min) = bucket
                    .iter()
                    .enumerate()
                    .min_by(|(_, x), (_, y)| x.1.total_cmp(&y.1))
                    .unwrap();
                let (max_i, max) = bucket
                    .iter()
                    .enumerate()
                    .max_by(|(_, x), (_, y)| x.1.total_cmp(&y.1))
                    .unwrap();
                match (min_i, max_i) {
                    (i, j) if i < j => [*min, *max],
                    _ => [*max, *min],
                }
            }
            Self::Mean => {
                [(0.0, Y::default()), (0.0, Y::default())] // TODO
            }
        }
    }
}

#[derive(Debug)]
pub struct TimeseriesLineMemory<X, Y> {
    downsampling_method: DownsamplingMethod,
    cached_data: Option<CacheDescriptor<X, Y>>,
    cache_levels: Vec<Vec<(f64, Y)>>,
    view_cache: Option<(PlotBounds, Vec<[f64; 2]>)>,
    // TODO: Fix x axis behaviour
    x_axis_origin: Option<X>,
}

impl<
        X: TimeseriesXAxis,
        Y: Default + num_traits::Float + num_traits::float::TotalOrder + Into<f64>,
    > TimeseriesLineMemory<X, Y>
{
    fn new(downsampling_method: DownsamplingMethod) -> Self {
        Self {
            downsampling_method,
            cached_data: None,
            cache_levels: Vec::new(),
            view_cache: None,
            x_axis_origin: None,
        }
    }

    pub fn clear_caches(&mut self) {
        self.cache_levels.truncate(1);
        if self.cache_levels.len() == 0 {
            self.cache_levels.push(Vec::new());
        } else {
            self.cache_levels[0].truncate(0);
        }
        // Also clear the cache descriptor so update_cache will rebuild from scratch
        self.cached_data = None;
        self.view_cache = None;
    }

    fn rebuild_caches<
        'a,
        I: Iterator<Item = (X, Option<Y>)> + ExactSizeIterator + DoubleEndedIterator + 'a,
    >(
        &mut self,
        data: I,
    ) {
        self.clear_caches();
        self.extend_caches(data);
    }

    fn extend_caches<
        'a,
        I: Iterator<Item = (X, Option<Y>)> + ExactSizeIterator + DoubleEndedIterator + 'a,
    >(
        &mut self,
        data: I,
    ) {
        //println!("extending caches");
        if self.cache_levels.len() == 0 {
            self.cache_levels.push(Vec::new());
        }

        //let len = data.len();
        //println!("starting first layer update");
        //let t = std::time::Instant::now();
        //self.cache_levels[0].extend(data.filter_map(|(t, y)| y.map(|y| [t.to_f64(&mut self.x_axis_origin), y])));
        //for (t, y) in data {
        //    let Some(y) = y else {
        //        continue;
        //    };
        //    let x = t.to_f64(&mut self.x_axis_origin);
        //    self.cache_levels[0].push([x, y]);
        //}

        let new = data.filter_map(|(t, y)| y.map(|y| (t.to_f64(&mut self.x_axis_origin), y)));
        self.cache_levels[0].extend(new);
        //println!("first-layer update: {:?} ({:?}/{:?})", t.elapsed(), len - skip, len);
        //println!("first-layer update: {:?} ({:?})", t.elapsed(), len);

        for i in 1..=MAX_DOWNSAMPLING_STEPS {
            if i >= self.cache_levels.len() {
                if self.cache_levels.last().unwrap().len() > MAX_POINTS {
                    self.cache_levels.push(Vec::new());
                } else {
                    break;
                }
            }

            let len = usize::max(self.cache_levels[i].len(), 2) - 2;
            self.cache_levels[i].truncate(len);

            let skip = self.cache_levels[i].len() * (DOWNSAMPLING_BUCKET_SIZE / 2);
            let new: Vec<(f64, Y)> = (&self.cache_levels[i - 1][skip..])
                .chunks(DOWNSAMPLING_BUCKET_SIZE)
                .map(|c| self.downsampling_method.downsample(c))
                .flatten()
                .collect();
            //println!("{:?}", new.len());
            self.cache_levels[i].extend(new);
        }
    }

    fn update_cache<
        'a,
        I: Iterator<Item = (X, Option<Y>)> + ExactSizeIterator + DoubleEndedIterator + 'a,
    >(
        &mut self,
        iterator: I,
    ) {
        //println!("updating cache");
        let data = iterator.map(|(x, y)| {
            //println!("{:?}", x);
            (x.clone(), y)
        });

        // This is a bit weird
        //let mut data = data.rev().peekable();
        //let last_element = data.peek().cloned();
        //let mut data = data.rev().peekable();
        let mut data = data.peekable();
        let first_element = data.peek().cloned();

        let new = CacheDescriptor {
            len: data.len(),
            first_data_point: first_element,
            //last_data_point: last_element,
        };

        if let Some(old) = self.cached_data.as_ref() {
            if new.len < old.len || new.first_data_point != old.first_data_point {
                self.rebuild_caches(data);
            } else {
                //match (new.len > old.len, new.last_data_point != old.last_data_point) {
                //    (true, _) => self.extend_caches(data.skip(old.len)),
                //    (false, true) => self.rebuild_caches(data),
                //    (false, false) => {}, // We're up to date
                //}
                if new.len > old.len {
                    self.extend_caches(data.skip(old.len));
                }
            }
        } else {
            self.rebuild_caches(data);
        }

        self.cached_data = Some(new);
    }

    fn end(&self) -> Option<f64> {
        self.cache_levels
            .get(0)
            .map(|c| c.last())
            .flatten()
            .map(|xy| xy.0)
    }

    fn plot(&mut self, plot_bounds: PlotBounds) -> Vec<[f64; 2]> {
        if self.cache_levels.len() == 0 {
            return Vec::new();
        }

        // See if we have already plotted those exact bounds last time
        if let Some((bounds, cached)) = self.view_cache.as_ref() {
            if bounds.min() == plot_bounds.min() && bounds.max() == plot_bounds.max() {
                return cached.clone();
            }
        }

        // If we haven't, try to find the appropriate cache level for the zoom
        let num_cache_levels = self.cache_levels.len();
        for (i, cache_level) in self.cache_levels.iter().enumerate() {
            // find beginning and end for the given plot bounds in the current
            // cache level by binary search.
            let (x_min, x_max) = (plot_bounds.min()[0], plot_bounds.max()[0]);
            let i_begin = usize::max(1, cache_level.partition_point(|v| v.0 < x_min)) - 1;
            let i_end = usize::min(
                cache_level.partition_point(|v| v.0 <= x_max) + 1,
                cache_level.len(),
            );

            // If the points in view are few enough, stop and plot them.
            // If not, keep going down the cache.
            let points = &cache_level[i_begin..i_end];
            if points.len() < MAX_POINTS || i == (num_cache_levels - 1) {
                let mut points: Vec<_> = points.iter().map(|(x, y)| [*x, (*y).into()]).collect();

                // We also add the very first and very last points to the plotted
                // data, even if they are not visible. This allows egui to
                // properly initialize the plot and adjust the initial plot bounds
                // to the plotted data.
                if i_begin > 0 {
                    // In order to not upset the auto Y scaling, we only use the
                    // X axis value and copy the Y axis from the previous first
                    // instead.
                    //
                    // This way we can still zoom in on some detail even if the
                    // first/last values have vastly different Y axis values.
                    let previous_first_y = points[0][1];
                    points.insert(0, [cache_level[0].0, previous_first_y.into()]);
                }

                if cache_level.len() > 1 && i_end < cache_level.len() - 1 && points.len() > 0 {
                    let previous_last_y = points[points.len() - 1][1];
                    points.push([cache_level[cache_level.len() - 1].0, previous_last_y]);
                }

                let points_f64: Vec<_> = points
                    .into_iter()
                    .map(|x| [x[0].into(), x[1].into()])
                    .collect();

                //if points.len() < 50 {
                //    println!("{:?}", points.iter().map(|p| p[0]).collect::<Vec<_>>());
                //}

                self.view_cache = Some((plot_bounds, points_f64.clone()));
                return points_f64;
            }
        }

        Vec::new()
    }
}

pub struct TimeseriesGroup {
    pub(crate) link_group_name: String,
    pub(crate) link_y: bool,
    pub(crate) last_view_width: Option<f64>,
}

impl TimeseriesGroup {
    pub fn new(name: impl ToString, link_y: bool) -> Self {
        Self {
            link_group_name: name.to_string(),
            link_y,
            last_view_width: None,
        }
    }
}

/// Main memory object for a timeseries plot.
///
/// Your application is expected to create this before a plot is shown for the
/// first time and keep it in memory until the plot is not needed anymore.
///
/// For drawing the actual plot, create a new [crate::TimeseriesPlot] struct
/// for each frame with a mutable reference to this struct.
///
/// This object contains various caches to speed up the drawing of large
/// timeseries. Depending on whether and how the plotted data changes during
/// the lifetime of this struct, changes are recognized automatically based
/// on the iterators supplied when drawing the plot:
///
/// - ### Appending new plot points to the end of the iterator
///
///   This is recognized automatically, and caches for already known points
///   are not discarded.
///
/// - ### Inserting new values at the start
///
///   This is recognized properly, but performance may suffer, as the entire
///   cache is rebuilt.
///
/// - ### Deleting values from the data
///
///   This should be recognized automatically. However, if you add and delete
///   the same number of values in a single frame, this would be equivalent to...
///
/// - ### Modifying values without changing the number of points
///
///   These changes may be missed, depending on which points are changed.
///   To save time, not every point is examined when deciding whether to rebuild
///   caches. If your usecase requires changes to the data, especially subtle
///   ones, calling [TimeseriesPlotMemory::clear_caches] when doing so is
///   recommended.
#[derive(Debug)]
pub struct TimeseriesPlotMemory<X, Y> {
    pub(crate) id: egui::Id,
    lines: HashMap<String, TimeseriesLineMemory<X, Y>>,
    downsampling_method: DownsamplingMethod,
    pub(crate) reset_auto_bounds_next_frame: bool,
    pub(crate) last_view_width: f64,
    pub(crate) last_auto_bounds: bool,
}

impl<
        X: TimeseriesXAxis,
        Y: Default + num_traits::Float + num_traits::float::TotalOrder + Into<f64>,
    > TimeseriesPlotMemory<X, Y>
{
    /// Create a new memory struct with a unique id.
    pub fn new<I: Into<egui::Id>>(id: I) -> Self {
        Self {
            id: id.into(),
            lines: HashMap::new(),
            downsampling_method: DownsamplingMethod::MinMax,
            reset_auto_bounds_next_frame: true,
            last_view_width: 10.0,
            last_auto_bounds: true,
        }
    }

    /// Most of the time, changes to the plotted data should be picked up
    /// automatically, but certain changes might be hard to detect, for
    /// instance if the length of the data and most of the content remains
    /// the same, while a single point in the middle changes.
    ///
    /// If you know your values have changed, for instance if your application
    /// has loaded a new file or something similar, you can help out by calling
    /// this method.
    pub fn clear_caches(&mut self) {
        for (_key, line) in self.lines.iter_mut() {
            line.clear_caches();
        }
    }

    /// Update the contained caches for the given line with the given iterator.
    ///
    /// This generally does not need to be called manually, since it is called
    /// by [crate::TimeseriesPlot] when needed.
    ///
    /// If you store your [TimeseriesPlotMemory] in something like a mutex, and
    /// your are processing new values in another thread, it may be possible to
    /// update the caches from the background thread in preparation for the next
    /// frame.
    pub fn update_cache<
        'a,
        I: Iterator<Item = (X, Option<Y>)> + ExactSizeIterator + DoubleEndedIterator + 'a,
    >(
        &mut self,
        line_id: &String,
        line_iterator: I,
    ) where
        X: 'a,
    {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if !self.lines.contains_key(line_id) {
            self.lines.insert(
                line_id.clone(),
                TimeseriesLineMemory::new(self.downsampling_method),
            );
        }

        self.lines
            .get_mut(line_id)
            .unwrap()
            .update_cache(line_iterator);
    }

    /// Returns the data to be plotted for the given line and current plot bounds.
    ///
    /// Called by [crate::TimeseriesPlot] when needed.
    pub fn plot(&mut self, line_id: &String, plot_bounds: PlotBounds) -> Vec<[f64; 2]> {
        self.lines
            .get_mut(line_id)
            .map(|l| l.plot(plot_bounds))
            .unwrap_or_default()
    }

    /// Returns the current last known X axis value, if any are present.
    pub fn end(&self) -> Option<f64> {
        let line_maxes: Vec<f64> = self.lines.values().filter_map(|l| l.end()).collect();
        (line_maxes.len() > 0).then_some(
            line_maxes
                .iter()
                .fold(f64::NEG_INFINITY, |a, b| f64::max(a, *b)),
        )
    }
}
