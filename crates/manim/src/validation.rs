use crate::protocol::{AnimationAction, ManimObject, ManimRenderRequest, ManimStyle, Position};
use engine_core::{EngineError, EngineResult};
use engine_math::{Expression, MathLimits, parse_expression};
use std::collections::{HashMap, HashSet};

const MAX_OBJECTS: usize = 1_000;
const MAX_STEPS: usize = 5_000;
const MAX_TEXT_CHARS: usize = 20_000;
const MAX_DURATION_SECONDS: f64 = 1_800.0;

pub fn validate_request(request: &ManimRenderRequest) -> EngineResult<()> {
    validate_output(request)?;
    if request
        .scene
        .title
        .as_ref()
        .is_some_and(|title| title.chars().count() > 500)
    {
        return Err(EngineError::capacity(
            "manim_title_too_long",
            "scene titles may contain at most 500 characters",
        ));
    }
    if request.scene.objects.is_empty() || request.scene.objects.len() > MAX_OBJECTS {
        return Err(EngineError::capacity(
            "manim_object_count_invalid",
            format!("a scene requires 1 to {MAX_OBJECTS} objects"),
        ));
    }
    if request.scene.timeline.len() > MAX_STEPS {
        return Err(EngineError::capacity(
            "manim_timeline_too_large",
            format!("a scene supports at most {MAX_STEPS} animation steps"),
        ));
    }
    validate_color(&request.scene.background_color)?;
    let mut objects = HashMap::new();
    for object in &request.scene.objects {
        validate_id(object.id())?;
        if objects.insert(object.id(), object).is_some() {
            return Err(EngineError::conflict(
                "manim_object_id_duplicate",
                format!("object id '{}' appears more than once", object.id()),
            ));
        }
        validate_object(object)?;
    }
    for object in &request.scene.objects {
        validate_object_references(object, &objects)?;
    }
    validate_group_cycles(&objects)?;
    let ids = objects.keys().copied().collect::<HashSet<_>>();
    let mut end = 0.0_f64;
    for step in &request.scene.timeline {
        if !step.start_seconds.is_finite()
            || !step.duration_seconds.is_finite()
            || step.start_seconds < 0.0
            || step.duration_seconds < 0.0
        {
            return Err(EngineError::invalid_input(
                "manim_timing_invalid",
                "animation timing must be finite and nonnegative",
            ));
        }
        end = end.max(step.start_seconds + step.duration_seconds);
        validate_action(&step.action, &ids)?;
    }
    if end > MAX_DURATION_SECONDS {
        return Err(EngineError::capacity(
            "manim_duration_too_long",
            "a Manim scene may be at most 30 minutes",
        ));
    }
    Ok(())
}

fn validate_output(request: &ManimRenderRequest) -> EngineResult<()> {
    let output = &request.output;
    if !(320..=7680).contains(&output.width)
        || !(240..=4320).contains(&output.height)
        || !(12..=120).contains(&output.frames_per_second)
        || !matches!(output.format.as_str(), "mp4" | "webm" | "gif")
        || output.transparent && output.format == "mp4"
    {
        return Err(EngineError::invalid_input(
            "manim_output_invalid",
            "output dimensions, frame rate, or format are unsupported",
        ));
    }
    Ok(())
}

fn validate_object(object: &ManimObject) -> EngineResult<()> {
    match object {
        ManimObject::Text {
            text,
            position,
            style,
            ..
        } => {
            if text.chars().count() > MAX_TEXT_CHARS {
                return Err(EngineError::capacity(
                    "manim_text_too_long",
                    "text object is too long",
                ));
            }
            validate_position(*position)?;
            validate_style(style)
        }
        ManimObject::MathExpression {
            expression,
            position,
            style,
            ..
        } => {
            expression.validate(MathLimits::default())?;
            validate_position(*position)?;
            validate_style(style)
        }
        ManimObject::Equation {
            equation,
            position,
            style,
            ..
        } => {
            equation.validate(MathLimits::default())?;
            validate_position(*position)?;
            validate_style(style)
        }
        ManimObject::Shape {
            position,
            width,
            height,
            style,
            ..
        } => {
            validate_position(*position)?;
            validate_positive_size(*width, *height)?;
            validate_style(style)
        }
        ManimObject::Axes2d {
            position,
            x_range,
            y_range,
            width,
            height,
            style,
            ..
        } => {
            validate_position(*position)?;
            validate_range(*x_range)?;
            validate_range(*y_range)?;
            validate_positive_size(*width, *height)?;
            validate_style(style)
        }
        ManimObject::Plot2d {
            expression,
            variable,
            domain,
            style,
            ..
        } => {
            expression.validate(MathLimits::default())?;
            validate_variable(variable)?;
            if expression.symbols().iter().any(|symbol| symbol != variable) {
                return Err(EngineError::invalid_input(
                    "manim_plot_symbol_invalid",
                    "plot expressions may only reference their declared variable",
                ));
            }
            validate_range(*domain)?;
            validate_style(style)
        }
        ManimObject::Group { members, .. } => {
            if members.is_empty() {
                return Err(EngineError::invalid_input(
                    "manim_group_empty",
                    "a group requires at least one member",
                ));
            }
            Ok(())
        }
    }
}

fn validate_object_references(
    object: &ManimObject,
    objects: &HashMap<&str, &ManimObject>,
) -> EngineResult<()> {
    match object {
        ManimObject::Plot2d { axes_id, .. } => {
            if !matches!(
                objects.get(axes_id.as_str()),
                Some(ManimObject::Axes2d { .. })
            ) {
                return Err(EngineError::not_found(
                    "manim_axes_reference_invalid",
                    format!("plot '{}' does not reference an Axes2d object", object.id()),
                ));
            }
        }
        ManimObject::Group { members, .. } => {
            for reference in members {
                if !objects.contains_key(reference.as_str()) || reference == object.id() {
                    return Err(EngineError::not_found(
                        "manim_object_reference_invalid",
                        format!(
                            "object '{}' has invalid reference '{reference}'",
                            object.id()
                        ),
                    ));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_group_cycles(objects: &HashMap<&str, &ManimObject>) -> EngineResult<()> {
    fn visit<'a>(
        id: &'a str,
        objects: &HashMap<&'a str, &'a ManimObject>,
        visiting: &mut HashSet<&'a str>,
        visited: &mut HashSet<&'a str>,
    ) -> EngineResult<()> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            return Err(EngineError::conflict(
                "manim_group_cycle",
                "groups must not contain a reference cycle",
            ));
        }
        if let Some(ManimObject::Group { members, .. }) = objects.get(id) {
            for member in members {
                visit(member, objects, visiting, visited)?;
            }
        }
        visiting.remove(id);
        visited.insert(id);
        Ok(())
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for id in objects.keys().copied() {
        visit(id, objects, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn validate_action(action: &AnimationAction, ids: &HashSet<&str>) -> EngineResult<()> {
    let references = match action {
        AnimationAction::Create { object_id }
        | AnimationAction::Write { object_id }
        | AnimationAction::FadeIn { object_id }
        | AnimationAction::FadeOut { object_id }
        | AnimationAction::MoveTo { object_id, .. }
        | AnimationAction::Highlight { object_id, .. } => vec![object_id.as_str()],
        AnimationAction::Transform { from_id, to_id } => vec![from_id.as_str(), to_id.as_str()],
        AnimationAction::Wait | AnimationAction::Camera { .. } => Vec::new(),
    };
    if references.iter().any(|reference| !ids.contains(reference)) {
        return Err(EngineError::not_found(
            "manim_action_reference_invalid",
            "animation action references an unknown object",
        ));
    }
    match action {
        AnimationAction::MoveTo { position, .. } => validate_position(*position)?,
        AnimationAction::Highlight { color, .. } => validate_color(color)?,
        AnimationAction::Camera { frame } => {
            validate_position(frame.center)?;
            if !frame.zoom.is_finite()
                || !(0.01..=100.0).contains(&frame.zoom)
                || !frame.rotation_degrees.is_finite()
                || frame.rotation_degrees.abs() > 36_000.0
            {
                return Err(EngineError::invalid_input(
                    "manim_camera_invalid",
                    "camera zoom is invalid",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_id(value: &str) -> EngineResult<()> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(EngineError::invalid_input(
            "manim_id_invalid",
            "object ids must be safe ASCII identifiers",
        ));
    }
    Ok(())
}

fn validate_variable(value: &str) -> EngineResult<()> {
    match parse_expression(value)? {
        Expression::Symbol { name } if name == value => Ok(()),
        _ => Err(EngineError::invalid_input(
            "manim_variable_invalid",
            "plot variable is invalid",
        )),
    }
}

fn validate_position(position: Position) -> EngineResult<()> {
    if [position.x, position.y, position.z]
        .into_iter()
        .any(|v| !v.is_finite() || v.abs() > 1.0e6)
    {
        return Err(EngineError::invalid_input(
            "manim_position_invalid",
            "positions must be finite and bounded",
        ));
    }
    Ok(())
}

fn validate_positive_size(width: f64, height: f64) -> EngineResult<()> {
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || width > 1.0e6
        || height > 1.0e6
    {
        return Err(EngineError::invalid_input(
            "manim_size_invalid",
            "object dimensions must be positive, finite, and bounded",
        ));
    }
    Ok(())
}

fn validate_range(range: (f64, f64)) -> EngineResult<()> {
    if !range.0.is_finite() || !range.1.is_finite() || range.0 >= range.1 {
        return Err(EngineError::invalid_input(
            "manim_range_invalid",
            "range minimum must be less than its maximum",
        ));
    }
    Ok(())
}

fn validate_style(style: &ManimStyle) -> EngineResult<()> {
    if !style.opacity.is_finite()
        || !(0.0..=1.0).contains(&style.opacity)
        || !style.stroke_width.is_finite()
        || !(0.0..=50.0).contains(&style.stroke_width)
        || !style.scale.is_finite()
        || !(0.01..=100.0).contains(&style.scale)
    {
        return Err(EngineError::invalid_input(
            "manim_style_invalid",
            "style values are outside supported bounds",
        ));
    }
    if let Some(color) = &style.color {
        validate_color(color)?;
    }
    if let Some(color) = &style.fill_color {
        validate_color(color)?;
    }
    Ok(())
}

fn validate_color(color: &str) -> EngineResult<()> {
    let hex = color.strip_prefix('#').is_some_and(|digits| {
        matches!(digits.len(), 3 | 4 | 6 | 8) && digits.chars().all(|c| c.is_ascii_hexdigit())
    });
    let token = !color.is_empty()
        && color.len() <= 32
        && color
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !hex && !token {
        return Err(EngineError::invalid_input(
            "manim_color_invalid",
            "colors must be hex values or theme tokens",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AnimationStep, Easing, ManimScene, RenderOutput};
    use engine_math::parse_expression;

    fn request() -> ManimRenderRequest {
        ManimRenderRequest {
            scene: ManimScene {
                title: Some("Parabola".into()),
                background_color: "background".into(),
                objects: vec![ManimObject::MathExpression {
                    id: "equation".into(),
                    expression: parse_expression("x^2").unwrap(),
                    position: Position {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    style: ManimStyle::default(),
                }],
                timeline: vec![AnimationStep {
                    start_seconds: 0.0,
                    duration_seconds: 1.0,
                    easing: Easing::Smooth,
                    action: AnimationAction::Write {
                        object_id: "equation".into(),
                    },
                }],
            },
            output: RenderOutput::default(),
        }
    }

    #[test]
    fn accepts_structured_math_without_python_or_latex() {
        validate_request(&request()).unwrap();
    }

    #[test]
    fn rejects_unknown_timeline_references() {
        let mut input = request();
        input.scene.timeline[0].action = AnimationAction::Create {
            object_id: "missing".into(),
        };
        assert_eq!(
            validate_request(&input).unwrap_err().code,
            "manim_action_reference_invalid"
        );
    }

    #[test]
    fn rejects_recursive_groups() {
        let mut input = request();
        input.scene.objects.extend([
            ManimObject::Group {
                id: "group-a".into(),
                members: vec!["group-b".into()],
            },
            ManimObject::Group {
                id: "group-b".into(),
                members: vec!["group-a".into()],
            },
        ]);
        assert_eq!(
            validate_request(&input).unwrap_err().code,
            "manim_group_cycle"
        );
    }
}
