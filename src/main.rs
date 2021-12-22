use lyon::geom::LineSegment;
use lyon::path::builder::*;
use lyon::path::math::{point, Point};
use lyon::path::{Event, Path, PathEvent};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serialization", derive(Serialize, Deserialize))]
struct DashOptions {
    pub initial_offset: f32,
    pub array: Vec<f32>,
}

impl DashOptions {
    pub fn new(initial_offset: f32, array: Vec<f32>) -> Self {
        assert!(!array.is_empty());
        assert_eq!(array.iter().enumerate().find(|(_, &x)| x <= 0.0), None);
        DashOptions {
            initial_offset,
            array,
        }
    }
}

// A distance-based cursor.
struct DashCursor {
    pub array: Vec<f32>,
    pub cumulative_array: Vec<f32>,
    pub initial_offset: f32,
    pub initial_index: usize,
    pub current_offset: f32,
    pub current_index: usize,
}

#[derive(PartialEq, Eq, Debug)]
enum DashActionType {
    Dash,
    Gap,
}

#[derive(Debug)]
struct DashAction {
    /// The length of the current dash segment.
    length: f32,
    /// Use the remaining_distance as an argument to progress_by.
    remaining_distance: f32,
    dash_action_type: DashActionType,
}

impl DashCursor {
    pub fn new(options: &DashOptions) -> Self {
        // TODO magic: duplicate if odd (needed?)
        // TODO magic: remove zeroes
        let cumulative_array = DashCursor::cumulate_array(&options.array);
        let current_offset = options
            .initial_offset
            .rem_euclid(*cumulative_array.last().unwrap()); // TODO does this work for negative offsets?
        let current_index =
            DashCursor::find_index_in_cumulative_array(current_offset, &cumulative_array);
        DashCursor {
            array: options.array.clone(),
            cumulative_array,
            initial_offset: current_offset,
            initial_index: current_index,
            current_offset: current_offset,
            current_index: current_index,
        }
    }

    pub fn reset(&mut self) -> () {
        self.current_offset = self.initial_offset;
        self.current_index = self.initial_index;
    }

    fn cumulate_array(array: &[f32]) -> Vec<f32> {
        array
            .iter()
            .scan(0.0, |acc, &x| {
                *acc = *acc + x;
                Some(*acc)
            })
            .collect()
    }

    fn find_index_in_cumulative_array(offset: f32, cumulative_array: &[f32]) -> usize {
        let mut current_index = 0;
        for &x in cumulative_array {
            if x > offset {
                break;
            }
            current_index += 1;
        }
        assert!(current_index < cumulative_array.len()); // TODO make numerically more stable by using the last element?
        current_index
    }

    fn make_dash_action_type(index: usize) -> DashActionType {
        if index % 2 == 0 {
            return DashActionType::Dash;
        } else {
            return DashActionType::Gap;
        }
    }

    pub fn progress_by(&mut self, progress_distance: f32) -> DashAction {
        // Try to progress by the given distance, or until the next segment delimiter.
        let distance_to_next = self.cumulative_array[self.current_index] - self.current_offset;
        if distance_to_next <= progress_distance {
            // We reached a segment delimiter before reaching the line end.
            let dash_length = distance_to_next;
            if self.current_index < self.cumulative_array.len() - 1 {
                self.current_offset = self.cumulative_array[self.current_index];
                self.current_index = self.current_index + 1;
            } else {
                // Reset the cycle
                self.current_index = 0;
                self.current_offset = 0.0;
            }
            return DashAction {
                length: dash_length,
                remaining_distance: progress_distance - distance_to_next,
                dash_action_type: DashCursor::make_dash_action_type(self.current_index + 1),
            };
        } else {
            // We reached the requested line end without reaching a segment delimiter.
            self.current_offset = self.current_offset + progress_distance;
            return DashAction {
                length: progress_distance,
                remaining_distance: 0.0,
                dash_action_type: DashCursor::make_dash_action_type(self.current_index),
            };
        }
    }
}
// TODO Clean up assert_approx_eq (maybe across lyon?)
fn assert_approx_eq(a: f32, b: f32, epsilon: f32) {
    if f32::abs(a - b) > epsilon {
        println!("{:?} != {:?}", a, b);
    }
    assert!((a - b).abs() <= epsilon);
}

fn assert_slice_approx_eq(a: &[f32], b: &[f32], epsilon: f32) {
    for i in 0..a.len() {
        if f32::abs(a[i] - b[i]) > epsilon {
            println!("{:?} != {:?}", a, b);
        }
        assert!((a[i] - b[i]).abs() <= epsilon);
    }
    assert_eq!(a.len(), b.len());
}

#[test]
fn test_cursor_construction() {
    for factor in vec![1.0f32, 0.01f32] {
        for phase in vec![-2, -1, 1, 2] {
            let options = DashOptions::new(
                0.05 - (phase as f32) * factor * 16.0,
                vec![factor * 10.0, factor * 1.0, factor * 2.0, factor * 3.0],
            );
            let cursor = DashCursor::new(&options);
            assert_slice_approx_eq(
                &vec![
                    factor * 10.0f32,
                    factor * 11.0f32,
                    factor * 13.0f32,
                    factor * 16.0f32,
                ],
                &cursor.cumulative_array,
                f32::EPSILON,
            );
            assert_approx_eq(0.05, cursor.current_offset, 0.000000001);
            assert_eq!(0, cursor.current_index);
        }
    }
}

fn assert_action_eq(expected_action: &DashAction, action: &DashAction) {
    assert_approx_eq(expected_action.length, action.length, 0.000000001);
    assert_approx_eq(
        expected_action.remaining_distance,
        action.remaining_distance,
        0.001,
    );
    assert_eq!(expected_action.dash_action_type, action.dash_action_type);
}

fn make_dash(length: f32, remaining_distance: f32) -> DashAction {
    DashAction {
        length: length,
        remaining_distance: remaining_distance,
        dash_action_type: DashActionType::Dash,
    }
}

fn make_gap(length: f32, remaining_distance: f32) -> DashAction {
    DashAction {
        length: length,
        remaining_distance: remaining_distance,
        dash_action_type: DashActionType::Gap,
    }
}

#[test]
fn test_no_segment_cross() {
    let options = DashOptions::new(0.0, vec![1.0, 2.0]);
    let mut cursor = DashCursor::new(&options);
    let action = &cursor.progress_by(0.5);
    assert_action_eq(&make_dash(0.5, 0.0), action);
}

#[test]
fn test_segment_cross() {
    let options = DashOptions::new(0.0, vec![1.0, 2.0]);
    let mut cursor = DashCursor::new(&options);
    let action = cursor.progress_by(1.5);
    assert_action_eq(&make_dash(1.0, 0.5), &action);
    let action = cursor.progress_by(action.remaining_distance);
    assert_action_eq(&make_gap(0.5, 0.0), &action);
}

#[derive(Debug)]
enum DashOrGap {
    Dash {
        from: Point,
        to: Point,
        distance: f32,
    },
    Gap {
        // TODO squeeze gaps?
        distance: f32,
    },
}

struct FlattenedEventIterator {
    cursor: DashCursor,
}

impl FlattenedEventIterator {
    pub fn new(options: &DashOptions) -> Self {
        FlattenedEventIterator {
            cursor: DashCursor::new(&options),
        }
    }

    fn handle_line(&mut self, from: Point, to: Point) {
        let line = LineSegment { from, to };
        let mut current_relative_distance = 0.0f32;
        let line_length = line.length();
        let mut remaining_distance = line_length;
        while remaining_distance > 0.0f32 {
            let action = self.cursor.progress_by(remaining_distance);
            let next_relative_distance = current_relative_distance + action.length;
            match action.dash_action_type {
                DashActionType::Dash => {
                    let segment = line.split_range(std::ops::Range {
                        start: current_relative_distance / line_length,
                        end: next_relative_distance / line_length,
                    });
                    let output = DashOrGap::Dash {
                        from: segment.from,
                        to: segment.to,
                        distance: segment.length(),
                    };
                    println!("Yield {:?}", output);
                }
                DashActionType::Gap => {
                    let output = DashOrGap::Gap {
                        distance: action.length,
                    };
                    println!("Yield {:?}", output);
                }
            }
            remaining_distance = action.remaining_distance;
            current_relative_distance = next_relative_distance;
        }
    }

    pub fn next_event(&mut self, event: PathEvent) -> () {
        match event {
            PathEvent::Begin { .. } => {
                self.cursor.reset();
            }
            PathEvent::Line { from, to } => {
                self.handle_line(from, to);
            }
            PathEvent::End {
                last,
                first,
                close: true,
            } => {
                self.handle_line(last, first);
            }
            PathEvent::Quadratic { .. } => {
                // TODO auto-flatten?
                panic!("FlattenedEventIterator cannot handle quadratic path event!");
            }
            PathEvent::Cubic { .. } => {
                // TODO auto-flatten?
                panic!("FlattenedEventIterator cannot handle cubic path event!");
            }
            _ => {}
        }
    }
}

fn main() {
    // Build a simple path.
    let mut builder = Path::builder();
    builder.begin(point(0.0, 0.0));
    builder.line_to(point(10.0, 0.0));
    builder.line_to(point(10.0, 10.0));
    builder.line_to(point(20.0, 10.0));
    builder.line_to(point(20.0, 1.5));
    // TODO test skipping of line segment
    // TODO some kind of move_to?
    builder.close();
    let path = builder.build();

    // expected segments:
    //   line 0:
    // - DashTo (0,0)-(2,0)
    // - GapTo  (2,0)-(3,0)
    // - DashTo (3,0)-(5,0)
    // - GapTo  (5,0)-(6,0)
    // - DashTo (6,0)-(8,0)
    // - GapTo  (8,0)-(9,0)
    // - DashTo (9,0)-(10,0)
    //   line 1:
    // - DashTo (10,0)-(10,1)
    // - GapTo  (10,1)-(10,2)
    // - DashTo (10,2)-(10,4)
    // - GapTo  (10,4)-(10,5)
    // - DashTo (10,5)-(10,7)
    // - GapTo  (10,7)-(10,8)
    // - DashTo (10,8)-(10,10)
    //   line 2:
    // - GapTo  (10,10)-(11,10)
    // - DashTo (11,10)-(13,10)
    // - GapTo  (13,10)-(14,10)
    // - DashTo (14,10)-(16,10)
    // - GapTo  (16,10)-(17,10)
    // - DashTo (17,10)-(19,10)
    // - GapTo  (19,10)-(20,10)
    //   line 3:
    // - DashTo (20,10)-(20,8)
    // - GapTo  (20,8)-(20,7)
    // - DashTo (20,7)-(20,5)
    // - GapTo  (20,5)-(20,4)
    // - DashTo (20,4)-(20,2)
    // - GapTo  (20,2)-(20,1.5)

    let options = DashOptions::new(0.0, vec![1.0, 2.0]);
    let mut it = FlattenedEventIterator::new(&options);
    for event in &path {
        it.next_event(event);
    }
}