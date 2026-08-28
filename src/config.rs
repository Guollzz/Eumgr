use log::Level::Info;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;
use crate::logs;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub language: String,
}

impl Config {
    // Generate default configuration.
    pub fn default() -> Self {
        Config {
            language: "zh-CN".to_string(),
        }
    }

    // Write full configuration.
    pub fn save(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        logs::logs_output::logs_output("config::load_or_create", "Configuration saved.", "info");
        Ok(())
    }

    // Read full configuration.
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&json)?;
        logs::logs_output::logs_output("config::load_or_create", "Full configuration loaded.", "info");
        Ok(config)
    }

    // Read or create (if not exists, create default).
    pub fn load_or_create(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        if Path::new(path).exists() {
            Config::load(path)
        } else {
            let config = Config::default();
            config.save(path)?;
            logs::logs_output::logs_output("config::load_or_create", "Default configuration created.", "info");
            Ok(config)
        }
    }

    // Method to read a specific field.

    // str
    pub fn get_string_field(path: &str, field: &str) -> Result<String, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let v: Value = serde_json::from_str(&json)?;
        
        match v[field].as_str() {
            Some(s) => Ok(s.to_string()),
            None => Err(format!("Field '{}' is not a string type or does not exist.", field).into()),
        }
    }

    // number
    pub fn get_u32_field(path: &str, field: &str) -> Result<u32, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let v: Value = serde_json::from_str(&json)?;
        
        match v[field].as_u64() {
            Some(n) => Ok(n as u32),
            None => Err(format!("Field '{}' is not a numeric type or does not exist.", field).into()),
        }
    }

    // bool
    pub fn get_bool_field(path: &str, field: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let v: Value = serde_json::from_str(&json)?;
        
        match v[field].as_bool() {
            Some(b) => Ok(b),
            None => Err(format!("Field '{}' is not a boolean type or does not exist.", field).into()),
        }
    }

    // Read array field.
    pub fn get_array_field(path: &str, field: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let v: Value = serde_json::from_str(&json)?;
        
        match v[field].as_array() {
            Some(arr) => {
                let mut result = Vec::new();
                for item in arr {
                    if let Some(s) = item.as_str() {
                        result.push(s.to_string());
                    }
                }
                Ok(result)
            }
            None => Err(format!("Field '{}' is not an array type or does not exist.", field).into()),
        }
    }

    // General method: read any field as Value.
    pub fn get_field(path: &str, field: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let v: Value = serde_json::from_str(&json)?;
        
        if let Some(value) = v.get(field) {
            Ok(value.clone())
        } else {
            Err(format!("Field '{}' does not exist.", field).into())
        }
    }
}