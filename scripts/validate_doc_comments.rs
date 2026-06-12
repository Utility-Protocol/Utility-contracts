#!/usr/bin/env rust-script

//! Doc-Comment Validation Script for IoT-Billing-Service Contracts
//! 
//! This script validates that all public functions, structs, and enums
//! have comprehensive documentation as required for audit readiness.
//! 
//! Usage: cargo run --bin validate_doc_comments

use std::collections::HashSet;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Validating doc-comment coverage for IoT-Billing-Service Contracts...\n");
    
    let contracts_dir = Path::new("contracts/utility_contracts/src");
    let mut total_items = 0;
    let mut documented_items = 0;
    let mut undocumented_items = Vec::new();
    
    // Validate main lib.rs
    validate_file(
        &contracts_dir.join("lib.rs"),
        &mut total_items,
        &mut documented_items,
        &mut undocumented_items,
    )?;
    
    // Validate nonce_sync module
    validate_file(
        &contracts_dir.join("nonce_sync.rs"),
        &mut total_items,
        &mut documented_items,
        &mut undocumented_items,
    )?;
    
    // Validate tariff_oracle module
    validate_file(
        &contracts_dir.join("tariff_oracle.rs"),
        &mut total_items,
        &mut documented_items,
        &mut undocumented_items,
    )?;
    
    // Validate ghost_sweeper module
    validate_file(
        &contracts_dir.join("ghost_sweeper.rs"),
        &mut total_items,
        &mut documented_items,
        &mut undocumented_items,
    )?;
    
    // Print results
    let coverage_percentage = if total_items > 0 {
        (documented_items as f64 / total_items as f64) * 100.0
    } else {
        0.0
    };
    
    println!("\n📊 Documentation Coverage Report:");
    println!("   Total items: {}", total_items);
    println!("   Documented: {}", documented_items);
    println!("   Coverage: {:.1}%", coverage_percentage);
    
    if !undocumented_items.is_empty() {
        println!("\n❌ Undocumented items:");
        for item in &undocumented_items {
            println!("   - {}", item);
        }
        println!("\n💡 Please add doc-comments to all undocumented items.");
        std::process::exit(1);
    } else {
        println!("\n✅ All items have proper documentation!");
        println!("🎉 Ready for security audit!");
    }
    
    Ok(())
}

fn validate_file(
    file_path: &Path,
    total_items: &mut usize,
    documented_items: &mut usize,
    undocumented_items: &mut Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !file_path.exists() {
        return Ok(());
    }
    
    println!("🔍 Checking: {}", file_path.display());
    
    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    
    let mut in_doc_comment = false;
    let mut current_item = None;
    let mut has_doc_comment = false;
    
    for (i, line) in lines.iter().enumerate() {
        let line = line.trim();
        
        // Detect public items
        if line.starts_with("pub struct ") || line.starts_with("pub enum ") || 
           line.starts_with("pub fn ") || line.starts_with("pub const ") ||
           line.starts_with("pub type ") || line.starts_with("pub trait ") {
            
            // Save previous item if exists
            if let Some(item) = current_item.take() {
                *total_items += 1;
                if has_doc_comment {
                    *documented_items += 1;
                } else {
                    undocumented_items.push(item);
                }
            }
            
            // Extract item name
            let item_name = extract_item_name(line);
            current_item = Some(format!("{}:{}", file_path.file_name().unwrap().to_string_lossy(), item_name));
            has_doc_comment = in_doc_comment;
            in_doc_comment = false;
        }
        
        // Detect doc comments
        if line.starts_with("///") || line.starts_with("//!") {
            in_doc_comment = true;
            if let Some(ref mut item) = current_item {
                has_doc_comment = true;
            }
        }
        
        // Reset doc comment flag on non-doc-comment lines
        if !line.is_empty() && !line.starts_with("///") && !line.starts_with("//!") && 
           !line.starts_with("pub") && !line.starts_with("#") && !line.starts_with("use") {
            in_doc_comment = false;
        }
    }
    
    // Handle last item
    if let Some(item) = current_item {
        *total_items += 1;
        if has_doc_comment {
            *documented_items += 1;
        } else {
            undocumented_items.push(item);
        }
    }
    
    Ok(())
}

fn extract_item_name(line: &str) -> String {
    if line.starts_with("pub struct ") {
        line.split("pub struct ").nth(1)
            .unwrap_or("")
            .split_whitespace().next()
            .unwrap_or("")
            .split('{').next()
            .unwrap_or("")
            .to_string()
    } else if line.starts_with("pub enum ") {
        line.split("pub enum ").nth(1)
            .unwrap_or("")
            .split_whitespace().next()
            .unwrap_or("")
            .split('{').next()
            .unwrap_or("")
            .to_string()
    } else if line.starts_with("pub fn ") {
        line.split("pub fn ").nth(1)
            .unwrap_or("")
            .split('(').next()
            .unwrap_or("")
            .to_string()
    } else if line.starts_with("pub const ") {
        line.split("pub const ").nth(1)
            .unwrap_or("")
            .split(':').next()
            .unwrap_or("")
            .to_string()
    } else if line.starts_with("pub type ") {
        line.split("pub type ").nth(1)
            .unwrap_or("")
            .split(';').next()
            .unwrap_or("")
            .split('=').next()
            .unwrap_or("")
            .trim()
            .to_string()
    } else if line.starts_with("pub trait ") {
        line.split("pub trait ").nth(1)
            .unwrap_or("")
            .split_whitespace().next()
            .unwrap_or("")
            .split('{').next()
            .unwrap_or("")
            .to_string()
    } else {
        "unknown".to_string()
    }
}
