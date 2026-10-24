//! Input-device listing from `getevent -lp` (the session's `device.txt`) and the `add device` / `name:` header
//! lines at the top of every capture.

/// One `ABS_*` axis as listed by `getevent -lp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsInfo {
    /// Axis value when the listing was taken.
    pub value: i32,
    pub min: i32,
    pub max: i32,
}

/// One `add device` block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InputDevice {
    /// e.g. `/dev/input/event10`.
    pub node: String,
    /// e.g. `sec_e-pen`.
    pub name: String,
    /// `(ABS_X, info)` pairs in listing order.
    pub abs: Vec<(String, AbsInfo)>,
}

/// Every device in a listing plus the `model:` line of `device.txt`, if any.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceListing {
    pub model: Option<String>,
    pub devices: Vec<InputDevice>,
}

impl DeviceListing {
    /// Lenient parse: unknown lines are ignored. A node listed twice (`device.txt` holds `-lp` then a short
    /// listing) merges into one entry.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut listing = Self::default();
        let mut current: Option<usize> = None;
        for line in text.lines() {
            if let Some(model) = line.strip_prefix("model:") {
                listing.model = Some(model.trim().to_owned());
            } else if let Some(node) = line
                .strip_prefix("add device ")
                .and_then(|r| r.split_once(": "))
            {
                current = Some(listing.device_index(node.1.trim()));
            } else if let Some(dev) = current.and_then(|i| listing.devices.get_mut(i)) {
                dev.read_line(line);
            }
        }
        listing
    }

    /// First device called `name`.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<&InputDevice> {
        self.devices.iter().find(|d| d.name == name)
    }

    fn device_index(&mut self, node: &str) -> usize {
        if let Some(i) = self.devices.iter().position(|d| d.node == node) {
            return i;
        }
        self.devices.push(InputDevice {
            node: node.to_owned(),
            ..InputDevice::default()
        });
        self.devices.len() - 1
    }
}

impl InputDevice {
    /// Info for axis `axis` (e.g. `"ABS_X"`).
    #[must_use]
    pub fn abs(&self, axis: &str) -> Option<AbsInfo> {
        self.abs
            .iter()
            .find(|(a, _)| a == axis)
            .map(|&(_, info)| info)
    }

    fn read_line(&mut self, line: &str) {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("name:") {
            name.trim().trim_matches('"').clone_into(&mut self.name);
        } else if let Some((axis, info)) = parse_abs(line)
            && self.abs(&axis).is_none()
        {
            self.abs.push((axis, info));
        }
    }
}

/// `ABS (0003): ABS_X : value 12809, min 0, max 14752, fuzz 4, …` or its continuation `ABS_Y : value …`.
fn parse_abs(line: &str) -> Option<(String, AbsInfo)> {
    let line = line
        .strip_prefix("ABS (0003):")
        .map_or(line, str::trim_start);
    if !line.starts_with("ABS_") {
        return None;
    }
    let (axis, fields) = line.split_once(':')?;
    let (mut value, mut min, mut max) = (None, None, None);
    for field in fields.split(',') {
        let mut kv = field.split_whitespace();
        let slot = match kv.next() {
            Some("value") => &mut value,
            Some("min") => &mut min,
            Some("max") => &mut max,
            _ => continue,
        };
        *slot = kv.next().and_then(|v| v.parse::<i32>().ok());
    }
    let info = AbsInfo {
        value: value?,
        min: min?,
        max: max?,
    };
    Some((axis.trim().to_owned(), info))
}

#[cfg(test)]
mod tests;
