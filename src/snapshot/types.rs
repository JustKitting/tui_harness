// Define core types for snapshot functionality

use chrono::{DateTime, Utc};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for snapshot capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    /// Directory where snapshots will be saved
    pub output_dir: PathBuf,

    /// Whether to include metadata JSON file
    pub include_metadata: bool,

    /// Whether to include manifest JSON file
    pub include_manifest: bool,

    /// Whether to allow mock captures when real display is not available (for testing only)
    pub allow_mock_captures: bool,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("./snapshots"),
            include_metadata: true,
            include_manifest: true,
            allow_mock_captures: false, // Default to production mode - no mocks
        }
    }
}

/// Represents a captured snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Path to the image file
    pub image_path: PathBuf,

    /// Source type (e.g., "display", "web")
    pub source: String,

    /// Optional metadata about the snapshot
    pub metadata: Option<serde_json::Value>,

    /// Timestamp when the snapshot was created
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
}

impl Snapshot {
    /// Create a new snapshot
    pub fn new(image_path: PathBuf, source: String, metadata: Option<serde_json::Value>) -> Self {
        Self {
            image_path,
            source,
            metadata,
            timestamp: Utc::now(),
        }
    }
}

/// Result type for snapshot operations
pub type SnapshotResult<T> = Result<T, SnapshotError>;

/// Error types for snapshot operations
#[derive(Debug)]
pub enum SnapshotError {
    /// Error during capture process
    Capture(String),

    /// I/O error
    Io(std::io::Error),

    /// Serialization error
    Serialization(serde_json::Error),
}

// Manual implementation of Serialize for SnapshotError
impl Serialize for SnapshotError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            SnapshotError::Capture(msg) => {
                let mut map = serializer.serialize_map(Some(1))?;
                SerializeMap::serialize_entry(&mut map, "Capture", msg)?;
                SerializeMap::end(map)
            }
            SnapshotError::Io(err) => {
                let mut map = serializer.serialize_map(Some(1))?;
                SerializeMap::serialize_entry(&mut map, "Io", &err.to_string())?;
                SerializeMap::end(map)
            }
            SnapshotError::Serialization(err) => {
                let mut map = serializer.serialize_map(Some(1))?;
                SerializeMap::serialize_entry(&mut map, "Serialization", &err.to_string())?;
                SerializeMap::end(map)
            }
        }
    }
}

// Manual implementation of Deserialize for SnapshotError
impl<'de> Deserialize<'de> for SnapshotError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct SnapshotErrorVisitor;

        impl<'de> Visitor<'de> for SnapshotErrorVisitor {
            type Value = SnapshotError;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("SnapshotError variant")
            }

            fn visit_map<V>(self, mut map: V) -> Result<SnapshotError, V::Error>
            where
                V: MapAccess<'de>,
            {
                let key = map
                    .next_key::<String>()?
                    .ok_or_else(|| de::Error::missing_field("variant"))?;
                match key.as_str() {
                    "Capture" => {
                        let value = map.next_value()?;
                        Ok(SnapshotError::Capture(value))
                    }
                    "Io" => {
                        let value: String = map.next_value()?;
                        Ok(SnapshotError::Io(std::io::Error::other(value)))
                    }
                    "Serialization" => {
                        let value: String = map.next_value()?;
                        // We can't reconstruct the original serde_json::Error, so we create a new one
                        // with the error message
                        Ok(SnapshotError::Serialization(serde_json::Error::io(
                            std::io::Error::other(value),
                        )))
                    }
                    _ => Err(de::Error::unknown_field(
                        &key,
                        &["Capture", "Io", "Serialization"],
                    )),
                }
            }
        }

        deserializer.deserialize_struct("SnapshotError", &[], SnapshotErrorVisitor)
    }
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::Capture(msg) => write!(f, "Capture error: {}", msg),
            SnapshotError::Io(err) => write!(f, "I/O error: {}", err),
            SnapshotError::Serialization(err) => write!(f, "Serialization error: {}", err),
        }
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SnapshotError::Capture(_) => None,
            SnapshotError::Io(err) => Some(err),
            SnapshotError::Serialization(err) => Some(err),
        }
    }
}

// Implement From traits for automatic error conversion
impl From<std::io::Error> for SnapshotError {
    fn from(err: std::io::Error) -> Self {
        SnapshotError::Io(err)
    }
}

impl From<serde_json::Error> for SnapshotError {
    fn from(err: serde_json::Error) -> Self {
        SnapshotError::Serialization(err)
    }
}

impl From<image::ImageError> for SnapshotError {
    fn from(err: image::ImageError) -> Self {
        SnapshotError::Io(std::io::Error::other(err.to_string()))
    }
}
