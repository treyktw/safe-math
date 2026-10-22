use engine_math::{Equation, Expression};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub z: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManimStyle {
    pub color: Option<String>,
    pub fill_color: Option<String>,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "default_stroke_width")]
    pub stroke_width: f32,
    #[serde(default = "default_scale")]
    pub scale: f32,
}

fn default_opacity() -> f32 {
    1.0
}
fn default_stroke_width() -> f32 {
    2.0
}
fn default_scale() -> f32 {
    1.0
}

impl Default for ManimStyle {
    fn default() -> Self {
        Self {
            color: None,
            fill_color: None,
            opacity: default_opacity(),
            stroke_width: default_stroke_width(),
            scale: default_scale(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Circle,
    Square,
    Rectangle,
    Triangle,
    Line,
    Arrow,
    Dot,
    Brace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManimObject {
    Text {
        id: String,
        text: String,
        position: Position,
        #[serde(default)]
        style: ManimStyle,
    },
    MathExpression {
        id: String,
        expression: Expression,
        position: Position,
        #[serde(default)]
        style: ManimStyle,
    },
    Equation {
        id: String,
        equation: Equation,
        position: Position,
        #[serde(default)]
        style: ManimStyle,
    },
    Shape {
        id: String,
        shape: ShapeKind,
        position: Position,
        width: f64,
        height: f64,
        #[serde(default)]
        style: ManimStyle,
    },
    Axes2d {
        id: String,
        position: Position,
        x_range: (f64, f64),
        y_range: (f64, f64),
        width: f64,
        height: f64,
        #[serde(default)]
        style: ManimStyle,
    },
    Plot2d {
        id: String,
        axes_id: String,
        expression: Expression,
        variable: String,
        domain: (f64, f64),
        #[serde(default)]
        style: ManimStyle,
    },
    Group {
        id: String,
        members: Vec<String>,
    },
}

impl ManimObject {
    pub fn id(&self) -> &str {
        match self {
            Self::Text { id, .. }
            | Self::MathExpression { id, .. }
            | Self::Equation { id, .. }
            | Self::Shape { id, .. }
            | Self::Axes2d { id, .. }
            | Self::Plot2d { id, .. }
            | Self::Group { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    Linear,
    Smooth,
    EaseIn,
    EaseOut,
    EaseInOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AnimationAction {
    Create {
        object_id: String,
    },
    Write {
        object_id: String,
    },
    FadeIn {
        object_id: String,
    },
    FadeOut {
        object_id: String,
    },
    Transform {
        from_id: String,
        to_id: String,
    },
    MoveTo {
        object_id: String,
        position: Position,
    },
    Highlight {
        object_id: String,
        color: String,
    },
    Wait,
    Camera {
        frame: CameraFrame,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraFrame {
    pub center: Position,
    pub zoom: f64,
    pub rotation_degrees: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationStep {
    pub start_seconds: f64,
    pub duration_seconds: f64,
    #[serde(default = "default_easing")]
    pub easing: Easing,
    pub action: AnimationAction,
}

fn default_easing() -> Easing {
    Easing::Smooth
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManimScene {
    pub title: Option<String>,
    #[serde(default = "default_background")]
    pub background_color: String,
    pub objects: Vec<ManimObject>,
    pub timeline: Vec<AnimationStep>,
}

fn default_background() -> String {
    "background".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderOutput {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_fps")]
    pub frames_per_second: u16,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default)]
    pub transparent: bool,
}

fn default_width() -> u32 {
    1920
}
fn default_height() -> u32 {
    1080
}
fn default_fps() -> u16 {
    30
}
fn default_format() -> String {
    "mp4".into()
}

impl Default for RenderOutput {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            frames_per_second: default_fps(),
            format: default_format(),
            transparent: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManimRenderRequest {
    pub scene: ManimScene,
    #[serde(default)]
    pub output: RenderOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderSubmission {
    pub job_id: String,
    pub status: String,
    pub renderer: String,
    /// Where the finished render landed, once it has.
    ///
    /// The renderer writes to the shared media volume rather than returning
    /// bytes: an animation that rendered successfully should survive a failure
    /// to upload it, and a multi-megabyte response body is a copy of a file
    /// that already exists.
    #[serde(default)]
    pub artifact: Option<RenderArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderArtifact {
    /// Relative to the media root. The renderer cannot know where the worker
    /// mounted the volume, so it does not pretend to.
    pub path: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub frames_per_second: u16,
    pub duration_seconds: f64,
    pub size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured verbatim from the renderer. The wire format is the contract
    /// between two languages, and the only way it stays one is by testing
    /// against something the other side actually said.
    const SUBMISSION: &str = r#"{
        "jobId": "9aa3bec3-873e-4c01-9c72-e84c93d39e75",
        "status": "completed",
        "renderer": "manim-0.19.0",
        "artifact": {
            "path": "renders/9aa3bec3-873e-4c01-9c72-e84c93d39e75/scene.mp4",
            "format": "mp4",
            "width": 854,
            "height": 480,
            "framesPerSecond": 30,
            "durationSeconds": 4.8,
            "sizeBytes": 53128
        }
    }"#;

    #[test]
    fn reads_what_the_renderer_answers_with() {
        let submission: RenderSubmission = serde_json::from_str(SUBMISSION).unwrap();
        assert_eq!(submission.status, "completed");
        let artifact = submission
            .artifact
            .expect("a completed render names its file");
        assert_eq!(
            artifact.path,
            "renders/9aa3bec3-873e-4c01-9c72-e84c93d39e75/scene.mp4"
        );
        assert_eq!(artifact.frames_per_second, 30);
        assert_eq!(artifact.size_bytes, 53_128);
    }

    #[test]
    fn a_submission_without_a_file_still_reads() {
        // An asynchronous renderer would answer this way, and the field was
        // added after the type existed. Neither should be a parse failure.
        let submission: RenderSubmission = serde_json::from_str(
            r#"{"jobId": "a", "status": "queued", "renderer": "manim-0.19.0"}"#,
        )
        .unwrap();
        assert!(submission.artifact.is_none());
    }

    #[test]
    fn a_scene_serialises_the_field_names_the_renderer_reads() {
        let request = ManimRenderRequest {
            scene: ManimScene {
                title: None,
                background_color: "background".into(),
                objects: vec![ManimObject::Plot2d {
                    id: "curve".into(),
                    axes_id: "axes".into(),
                    expression: engine_math::parse_expression("x").unwrap(),
                    variable: "x".into(),
                    domain: (-1.0, 1.0),
                    style: ManimStyle::default(),
                }],
                timeline: vec![AnimationStep {
                    start_seconds: 0.0,
                    duration_seconds: 1.0,
                    easing: Easing::Smooth,
                    action: AnimationAction::Create {
                        object_id: "curve".into(),
                    },
                }],
            },
            output: RenderOutput::default(),
        };
        let wire = serde_json::to_value(&request).unwrap();

        // serde renames enum variants, not the fields inside them. The
        // renderer has to read both spellings because of it, and this is the
        // test that says which one it will actually receive.
        assert_eq!(wire["scene"]["objects"][0]["axes_id"], "axes");
        assert_eq!(wire["scene"]["timeline"][0]["action"]["object_id"], "curve");
        assert_eq!(wire["scene"]["timeline"][0]["startSeconds"], 0.0);
        assert_eq!(wire["scene"]["backgroundColor"], "background");
        assert_eq!(wire["output"]["framesPerSecond"], 30);
    }
}
