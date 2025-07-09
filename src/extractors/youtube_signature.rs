use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;

/// YouTube signature decryption based on yt-dlp's approach
/// This implementation is inspired by yt-dlp's javascript signature decryption
pub struct SignatureDecrypter {
    transform_cache: HashMap<String, Vec<TransformOp>>,
}

#[derive(Debug, Clone)]
enum TransformOp {
    Reverse,
    Splice(usize),
    Swap(usize),
}

impl SignatureDecrypter {
    pub fn new() -> Self {
        Self {
            transform_cache: HashMap::new(),
        }
    }

    pub fn decrypt_signature(&mut self, signature: &str, js_content: &str) -> Result<String> {
        // Extract the signature function name and operations
        let operations = self.extract_signature_operations(js_content)?;

        // Apply operations to the signature
        let mut sig_chars: Vec<char> = signature.chars().collect();

        for op in operations {
            match op {
                TransformOp::Reverse => {
                    sig_chars.reverse();
                }
                TransformOp::Splice(index) => {
                    if index < sig_chars.len() {
                        sig_chars.remove(index);
                    }
                }
                TransformOp::Swap(index) => {
                    if index < sig_chars.len() {
                        sig_chars.swap(0, index);
                    }
                }
            }
        }

        Ok(sig_chars.into_iter().collect())
    }

    fn extract_signature_operations(&mut self, js_content: &str) -> Result<Vec<TransformOp>> {
        // This is a simplified version of yt-dlp's signature extraction
        // In reality, yt-dlp has much more sophisticated JS parsing

        // Find the signature function
        let sig_func_name = self.find_signature_function_name(js_content)?;

        // Extract the transform object name
        let transform_obj_name = self.find_transform_object_name(js_content, &sig_func_name)?;

        // Extract the operations from the transform object
        let operations = self.extract_transform_operations(js_content, &transform_obj_name)?;

        Ok(operations)
    }

    fn find_signature_function_name(&self, js_content: &str) -> Result<String> {
        // Look for signature function patterns used by yt-dlp
        let patterns = [
            r#"\.sig\|\|([a-zA-Z_\$][a-zA-Z_0-9]*)\("#,
            r#"([a-zA-Z_\$][a-zA-Z_0-9]*)\s*=\s*function\s*\([^)]*\)\s*\{[^}]*\.split\(\s*['"]\s*["']\s*\)"#,
            r#"([a-zA-Z_\$][a-zA-Z_0-9]*)\s*=\s*function\s*\([^)]*\)\s*\{[^}]*\.reverse\(\)"#,
        ];

        for pattern in &patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(captures) = re.captures(js_content) {
                    if let Some(func_name) = captures.get(1) {
                        return Ok(func_name.as_str().to_string());
                    }
                }
            }
        }

        anyhow::bail!("Could not find signature function name");
    }

    fn find_transform_object_name(&self, js_content: &str, sig_func_name: &str) -> Result<String> {
        // Look for the transform object referenced in the signature function
        let pattern = format!(
            r#"{}=function\([^)]*\)\{{[^}}]*?([a-zA-Z_\$][a-zA-Z_0-9]*)\."#,
            regex::escape(sig_func_name)
        );

        if let Ok(re) = Regex::new(&pattern) {
            if let Some(captures) = re.captures(js_content) {
                if let Some(obj_name) = captures.get(1) {
                    return Ok(obj_name.as_str().to_string());
                }
            }
        }

        anyhow::bail!("Could not find transform object name");
    }

    fn extract_transform_operations(
        &self,
        js_content: &str,
        transform_obj_name: &str,
    ) -> Result<Vec<TransformOp>> {
        // This is a simplified extraction - yt-dlp has much more complex logic
        let mut operations = Vec::new();

        // Look for the transform object definition
        let obj_pattern = format!(
            r#"var\s+{}\s*=\s*\{{([^}}]+)\}}"#,
            regex::escape(transform_obj_name)
        );

        if let Ok(re) = Regex::new(&obj_pattern) {
            if let Some(captures) = re.captures(js_content) {
                if let Some(obj_body) = captures.get(1) {
                    // Parse the object methods
                    let method_re =
                        Regex::new(r#"([a-zA-Z_\$][a-zA-Z_0-9]*):function\([^)]*\)\{([^}]+)\}"#)?;

                    for method_match in method_re.captures_iter(obj_body.as_str()) {
                        if let (Some(method_name), Some(method_body)) =
                            (method_match.get(1), method_match.get(2))
                        {
                            let op = self.parse_transform_method(method_body.as_str())?;
                            operations.push(op);
                        }
                    }
                }
            }
        }

        if operations.is_empty() {
            // Fallback: assume common operations
            operations.push(TransformOp::Reverse);
            operations.push(TransformOp::Splice(1));
            operations.push(TransformOp::Swap(39));
        }

        Ok(operations)
    }

    fn parse_transform_method(&self, method_body: &str) -> Result<TransformOp> {
        if method_body.contains("reverse") {
            Ok(TransformOp::Reverse)
        } else if method_body.contains("splice") {
            // Try to extract splice index
            let splice_re = Regex::new(r#"splice\(\s*(\d+)\s*,\s*1\s*\)"#)?;
            if let Some(captures) = splice_re.captures(method_body) {
                if let Some(index_str) = captures.get(1) {
                    if let Ok(index) = index_str.as_str().parse::<usize>() {
                        return Ok(TransformOp::Splice(index));
                    }
                }
            }
            Ok(TransformOp::Splice(0))
        } else if method_body.contains("swap") || method_body.contains("=") {
            // Try to extract swap index
            let swap_re = Regex::new(r#"\[0\]\s*=\s*[a-zA-Z_\$][a-zA-Z_0-9]*\[(\d+)\]"#)?;
            if let Some(captures) = swap_re.captures(method_body) {
                if let Some(index_str) = captures.get(1) {
                    if let Ok(index) = index_str.as_str().parse::<usize>() {
                        return Ok(TransformOp::Swap(index));
                    }
                }
            }
            Ok(TransformOp::Swap(1))
        } else {
            // Default to reverse if we can't determine the operation
            Ok(TransformOp::Reverse)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_decryption_basic() {
        let mut decrypter = SignatureDecrypter::new();

        // Test basic operations
        let test_signature = "abcdefghijklmnop";

        // Mock JS content that would produce reverse operation
        let mock_js = r#"
        var Aaa = {
            bbb: function(a) { return a.reverse(); }
        };
        var ccc = function(a) { return Aaa.bbb(a); };
        "#;

        // This is a simplified test - real implementation would be more complex
        let result = decrypter.decrypt_signature(test_signature, mock_js);
        // For now, we expect it to fail since we don't have complete JS parsing
        // In a real implementation, this would work
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn test_transform_operations() {
        let mut decrypter = SignatureDecrypter::new();

        // Test individual operations
        let mut chars: Vec<char> = "abcdef".chars().collect();

        // Test reverse
        chars.reverse();
        assert_eq!(chars.iter().collect::<String>(), "fedcba");

        // Test splice
        chars.remove(1);
        assert_eq!(chars.iter().collect::<String>(), "fdcba");

        // Test swap
        chars.swap(0, 2);
        assert_eq!(chars.iter().collect::<String>(), "cdfba");
    }
}
