use std::str::FromStr;

/// One organizer/event policy entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllowedTarget {
    Event { organizer: String, event: String },
    AllEvents { organizer: String },
}

impl FromStr for AllowedTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (organizer, event) = value
            .split_once('/')
            .ok_or_else(|| "expected ORGANIZER/EVENT or ORGANIZER/*".to_owned())?;
        if organizer.is_empty() || event.is_empty() || event.contains('/') {
            return Err(
                "organizer and event slugs must be non-empty and contain no '/'".to_owned(),
            );
        }

        if event == "*" {
            Ok(Self::AllEvents {
                organizer: organizer.to_owned(),
            })
        } else {
            Ok(Self::Event {
                organizer: organizer.to_owned(),
                event: event.to_owned(),
            })
        }
    }
}
