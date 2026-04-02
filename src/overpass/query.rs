use crate::Error;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};
use std::str::FromStr;

/// Represents a geographic bounding box (south, west, north, east)
/// Format: "28.8,-96.2,30.4,-94.7"
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

impl BoundingBox {
    pub fn new(south: f64, west: f64, north: f64, east: f64) -> Self {
        Self {
            south,
            west,
            north,
            east,
        }
    }

    /// Validate that the bounding box is well-formed
    pub fn validate(&self) -> Result<(), Error> {
        if self.south >= self.north {
            return Err(Error::InvalidBoundingBox(format!(
                "South ({}) must be less than North ({})",
                self.south, self.north
            )));
        }
        if self.west >= self.east {
            return Err(Error::InvalidBoundingBox(format!(
                "West ({}) must be less than East ({})",
                self.west, self.east
            )));
        }
        if self.south < -90.0 || self.south > 90.0 {
            return Err(Error::InvalidBoundingBox(format!(
                "South latitude ({}) must be between -90 and 90",
                self.south
            )));
        }
        if self.north < -90.0 || self.north > 90.0 {
            return Err(Error::InvalidBoundingBox(format!(
                "North latitude ({}) must be between -90 and 90",
                self.north
            )));
        }
        if self.west < -180.0 || self.west > 180.0 {
            return Err(Error::InvalidBoundingBox(format!(
                "West longitude ({}) must be between -180 and 180",
                self.west
            )));
        }
        if self.east < -180.0 || self.east > 180.0 {
            return Err(Error::InvalidBoundingBox(format!(
                "East longitude ({}) must be between -180 and 180",
                self.east
            )));
        }
        Ok(())
    }
}

impl FromStr for BoundingBox {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 4 {
            return Err(Error::InvalidBoundingBox(format!(
                "Expected 4 comma-separated values, got {}",
                parts.len()
            )));
        }

        let south = parts[0]
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::InvalidBoundingBox(format!("Invalid south value: {}", parts[0])))?;
        let west = parts[1]
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::InvalidBoundingBox(format!("Invalid west value: {}", parts[1])))?;
        let north = parts[2]
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::InvalidBoundingBox(format!("Invalid north value: {}", parts[2])))?;
        let east = parts[3]
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::InvalidBoundingBox(format!("Invalid east value: {}", parts[3])))?;

        let bbox = BoundingBox::new(south, west, north, east);
        bbox.validate()?;
        Ok(bbox)
    }
}

impl Display for BoundingBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{},{},{},{}",
            self.south, self.west, self.north, self.east
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounding_box_from_str() {
        let bbox = BoundingBox::from_str("28.8,-96.2,30.4,-94.7").unwrap();
        assert_eq!(bbox.south, 28.8);
        assert_eq!(bbox.west, -96.2);
        assert_eq!(bbox.north, 30.4);
        assert_eq!(bbox.east, -94.7);
    }

    #[test]
    fn test_bounding_box_to_string() {
        let bbox = BoundingBox::new(28.8, -96.2, 30.4, -94.7);
        assert_eq!(bbox.to_string(), "28.8,-96.2,30.4,-94.7");
    }

    #[test]
    fn test_bounding_box_invalid_count() {
        assert!(BoundingBox::from_str("28.8,-96.2,30.4").is_err());
        assert!(BoundingBox::from_str("28.8,-96.2,30.4,-94.7,extra").is_err());
    }

    #[test]
    fn test_bounding_box_invalid_values() {
        assert!(BoundingBox::from_str("invalid,-96.2,30.4,-94.7").is_err());
        assert!(BoundingBox::from_str("28.8,invalid,30.4,-94.7").is_err());
    }

    #[test]
    fn test_bounding_box_validation() {
        // South >= North
        assert!(BoundingBox::from_str("30.4,-96.2,28.8,-94.7").is_err());

        // West >= East
        assert!(BoundingBox::from_str("28.8,-94.7,30.4,-96.2").is_err());

        // Out of range latitudes
        assert!(BoundingBox::from_str("95.0,-96.2,30.4,-94.7").is_err());
        assert!(BoundingBox::from_str("28.8,-96.2,95.0,-94.7").is_err());

        // Out of range longitudes
        assert!(BoundingBox::from_str("28.8,-200.0,30.4,-94.7").is_err());
        assert!(BoundingBox::from_str("28.8,-96.2,30.4,200.0").is_err());
    }
}
