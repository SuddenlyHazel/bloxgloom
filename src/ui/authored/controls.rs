//! Closed editable values shared by schema, session validation and drawing.
use super::*;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectOption {
    pub(crate) key: String,
    pub(crate) label: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) enum Control {
    #[default]
    None,
    Checkbox {
        checked: bool,
    },
    Slider {
        value: f64,
        min: f64,
        max: f64,
        step: Option<f64>,
    },
    Select {
        options: Vec<SelectOption>,
        selected: String,
    },
}
impl Control {
    pub(crate) fn initial(&self, _kind: Kind, text: &str) -> String {
        match self {
            Self::None => text.into(),
            Self::Checkbox { checked } => checked.to_string(),
            Self::Slider { value, .. } => Self::number(*value),
            Self::Select { selected, .. } => selected.clone(),
        }
    }
    pub(crate) fn validate(&self, kind: Kind, value: &str) -> bool {
        if value.len() > Self::limit(kind)
            || value
                .chars()
                .any(|c| c.is_control() && !(kind == Kind::MultilineInput && c == '\n'))
        {
            return false;
        }
        match self {
            Self::None => matches!(kind, Kind::Input | Kind::MultilineInput),
            Self::Checkbox { .. } => matches!(value, "true" | "false"),
            Self::Select { options, .. } => options.iter().any(|option| option.key == value),
            Self::Slider { min, max, step, .. } => value.parse::<f64>().is_ok_and(|number| {
                number.is_finite()
                    && number >= *min
                    && number <= *max
                    && step.is_none_or(|step| {
                        let n = (number - min) / step;
                        number == *max || (n - n.round()).abs() <= 1e-8
                    })
            }),
        }
    }
    pub(crate) fn limit(kind: Kind) -> usize {
        if kind == Kind::MultilineInput {
            1024
        } else {
            MAX_TEXT
        }
    }
    pub(crate) fn number(value: f64) -> String {
        let text = value.to_string();
        if text.len() <= MAX_TEXT {
            text
        } else {
            format!("{value:e}")
        }
    }
    pub(crate) fn display(&self, value: &str) -> String {
        match self {
            Self::Checkbox { .. } => if value == "true" { "[x]" } else { "[ ]" }.into(),
            Self::Select { options, .. } => options
                .iter()
                .find(|o| o.key == value)
                .map_or_else(|| value.into(), |o| o.label.clone()),
            _ => value.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_values_preserve_byte_limits_ranges_and_declared_choices() {
        assert!(Control::None.validate(Kind::MultilineInput, "hello\n世界"));
        assert!(!Control::None.validate(Kind::Input, "hello\nworld"));
        assert!(!Control::None.validate(Kind::MultilineInput, "hello\tworld"));
        assert!(!Control::None.validate(Kind::MultilineInput, &"é".repeat(513)));
        let checkbox = Control::Checkbox { checked: false };
        assert!(checkbox.validate(Kind::Checkbox, "true"));
        assert!(!checkbox.validate(Kind::Checkbox, "1"));
        let slider = Control::Slider {
            value: 0.5,
            min: 0.0,
            max: 1.0,
            step: Some(0.25),
        };
        for value in ["0", "0.25", "0.5", "1"] {
            assert!(slider.validate(Kind::Slider, value));
        }
        for value in ["NaN", "inf", "-0.25", "1.25", "0.3"] {
            assert!(!slider.validate(Kind::Slider, value));
        }
        let select = Control::Select {
            options: vec![SelectOption {
                key: "stable".into(),
                label: "Stable".into(),
            }],
            selected: "stable".into(),
        };
        assert!(select.validate(Kind::Select, "stable"));
        assert!(!select.validate(Kind::Select, "Stable"));
        let number = Control::number(f64::MAX);
        assert!(number.len() <= MAX_TEXT);
        assert_eq!(number.parse::<f64>().unwrap(), f64::MAX);
    }
}
