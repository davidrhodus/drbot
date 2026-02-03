//! Project structure and language detection.

use crate::{DevToolsConfig, DevToolsError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Detected programming language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    Kotlin,
    Swift,
    CSharp,
    Cpp,
    C,
    Ruby,
    PHP,
    Scala,
    Haskell,
    Elixir,
    Dart,
    Lua,
    Shell,
    Markdown,
    Unknown,
}

/// Detected framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Framework {
    // Rust
    Actix,
    Axum,
    Rocket,
    Tokio,
    // JS/TS
    React,
    NextJS,
    Vue,
    Angular,
    Express,
    NestJS,
    // Python
    Django,
    Flask,
    FastAPI,
    // Go
    Gin,
    Echo,
    // Java
    Spring,
    // Other
    Rails,
    Phoenix,
    Flutter,
}

/// Build system detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildSystem {
    Cargo,
    Npm,
    Yarn,
    Pnpm,
    Pip,
    Poetry,
    GoMod,
    Maven,
    Gradle,
    Make,
    CMake,
    Bazel,
}

/// Project information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Detected languages.
    pub languages: HashSet<Language>,
    /// Detected frameworks.
    pub frameworks: HashSet<Framework>,
    /// Build systems.
    pub build_systems: HashSet<BuildSystem>,
    /// Entry points (main files).
    pub entry_points: Vec<PathBuf>,
    /// Configuration files found.
    pub config_files: Vec<PathBuf>,
    /// README path.
    pub readme_path: Option<PathBuf>,
    /// License type.
    pub license: Option<String>,
    /// Project name (from manifest).
    pub name: Option<String>,
    /// Project description.
    pub description: Option<String>,
}

/// Analyze a project.
pub async fn analyze_project(path: &Path, config: &DevToolsConfig) -> Result<ProjectInfo> {
    let mut info = ProjectInfo::default();

    // Check for various project files
    check_rust_project(path, &mut info)?;
    check_node_project(path, &mut info)?;
    check_python_project(path, &mut info)?;
    check_go_project(path, &mut info)?;
    check_java_project(path, &mut info)?;

    // Find README
    for name in &["README.md", "README.rst", "README.txt", "README"] {
        let readme_path = path.join(name);
        if readme_path.exists() {
            info.readme_path = Some(readme_path);
            break;
        }
    }

    // Find license
    for name in &["LICENSE", "LICENSE.md", "LICENSE.txt"] {
        let license_path = path.join(name);
        if license_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&license_path) {
                if content.contains("MIT License") {
                    info.license = Some("MIT".to_string());
                } else if content.contains("Apache License") {
                    info.license = Some("Apache-2.0".to_string());
                } else if content.contains("GNU GENERAL PUBLIC LICENSE") {
                    info.license = Some("GPL".to_string());
                }
            }
            break;
        }
    }

    Ok(info)
}

fn check_rust_project(path: &Path, info: &mut ProjectInfo) -> Result<()> {
    let cargo_toml = path.join("Cargo.toml");
    if cargo_toml.exists() {
        info.languages.insert(Language::Rust);
        info.build_systems.insert(BuildSystem::Cargo);
        info.config_files.push(cargo_toml.clone());

        // Check for main entry points
        for entry in &["src/main.rs", "src/lib.rs"] {
            let entry_path = path.join(entry);
            if entry_path.exists() {
                info.entry_points.push(entry_path);
            }
        }

        // Parse Cargo.toml for name and frameworks
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            if content.contains("tokio") {
                info.frameworks.insert(Framework::Tokio);
            }
            if content.contains("axum") {
                info.frameworks.insert(Framework::Axum);
            }
            if content.contains("actix") {
                info.frameworks.insert(Framework::Actix);
            }
            if content.contains("rocket") {
                info.frameworks.insert(Framework::Rocket);
            }

            // Extract name
            for line in content.lines() {
                if line.starts_with("name = ") {
                    info.name = Some(line.replace("name = ", "").trim_matches('"').to_string());
                    break;
                }
            }
        }
    }

    Ok(())
}

fn check_node_project(path: &Path, info: &mut ProjectInfo) -> Result<()> {
    let package_json = path.join("package.json");
    if package_json.exists() {
        info.config_files.push(package_json.clone());

        // Determine package manager
        if path.join("pnpm-lock.yaml").exists() {
            info.build_systems.insert(BuildSystem::Pnpm);
        } else if path.join("yarn.lock").exists() {
            info.build_systems.insert(BuildSystem::Yarn);
        } else if path.join("package-lock.json").exists() {
            info.build_systems.insert(BuildSystem::Npm);
        }

        // Check for TypeScript
        if path.join("tsconfig.json").exists() {
            info.languages.insert(Language::TypeScript);
            info.config_files.push(path.join("tsconfig.json"));
        } else {
            info.languages.insert(Language::JavaScript);
        }

        // Parse package.json for frameworks
        if let Ok(content) = std::fs::read_to_string(&package_json) {
            if content.contains("\"react\"") {
                info.frameworks.insert(Framework::React);
            }
            if content.contains("\"next\"") {
                info.frameworks.insert(Framework::NextJS);
            }
            if content.contains("\"vue\"") {
                info.frameworks.insert(Framework::Vue);
            }
            if content.contains("\"@angular\"") {
                info.frameworks.insert(Framework::Angular);
            }
            if content.contains("\"express\"") {
                info.frameworks.insert(Framework::Express);
            }
            if content.contains("\"@nestjs\"") {
                info.frameworks.insert(Framework::NestJS);
            }

            // Extract name
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(name) = json.get("name").and_then(|v| v.as_str()) {
                    info.name = Some(name.to_string());
                }
                if let Some(desc) = json.get("description").and_then(|v| v.as_str()) {
                    info.description = Some(desc.to_string());
                }
            }
        }
    }

    Ok(())
}

fn check_python_project(path: &Path, info: &mut ProjectInfo) -> Result<()> {
    // Check for various Python project files
    let pyproject = path.join("pyproject.toml");
    let setup_py = path.join("setup.py");
    let requirements = path.join("requirements.txt");

    if pyproject.exists() || setup_py.exists() || requirements.exists() {
        info.languages.insert(Language::Python);

        if pyproject.exists() {
            info.config_files.push(pyproject.clone());
            if let Ok(content) = std::fs::read_to_string(&pyproject) {
                if content.contains("[tool.poetry]") {
                    info.build_systems.insert(BuildSystem::Poetry);
                } else {
                    info.build_systems.insert(BuildSystem::Pip);
                }

                // Check frameworks
                if content.contains("django") {
                    info.frameworks.insert(Framework::Django);
                }
                if content.contains("flask") {
                    info.frameworks.insert(Framework::Flask);
                }
                if content.contains("fastapi") {
                    info.frameworks.insert(Framework::FastAPI);
                }
            }
        } else {
            info.build_systems.insert(BuildSystem::Pip);
        }

        if requirements.exists() {
            info.config_files.push(requirements);
        }
    }

    Ok(())
}

fn check_go_project(path: &Path, info: &mut ProjectInfo) -> Result<()> {
    let go_mod = path.join("go.mod");
    if go_mod.exists() {
        info.languages.insert(Language::Go);
        info.build_systems.insert(BuildSystem::GoMod);
        info.config_files.push(go_mod.clone());

        if let Ok(content) = std::fs::read_to_string(&go_mod) {
            if content.contains("gin-gonic") {
                info.frameworks.insert(Framework::Gin);
            }
            if content.contains("labstack/echo") {
                info.frameworks.insert(Framework::Echo);
            }
        }
    }

    Ok(())
}

fn check_java_project(path: &Path, info: &mut ProjectInfo) -> Result<()> {
    let pom = path.join("pom.xml");
    let gradle = path.join("build.gradle");
    let gradle_kts = path.join("build.gradle.kts");

    if pom.exists() {
        info.languages.insert(Language::Java);
        info.build_systems.insert(BuildSystem::Maven);
        info.config_files.push(pom);
    }

    if gradle.exists() || gradle_kts.exists() {
        info.build_systems.insert(BuildSystem::Gradle);

        if gradle_kts.exists() {
            info.languages.insert(Language::Kotlin);
            info.config_files.push(gradle_kts);
        } else {
            info.languages.insert(Language::Java);
            info.config_files.push(gradle);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_analyze_current_dir() {
        let config = DevToolsConfig::default();
        let result = analyze_project(Path::new("."), &config).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_language_enum() {
        let lang = Language::Rust;
        assert_eq!(format!("{:?}", lang), "Rust");
    }
}
