use anyhow::Result;
use rquickjs::{Context, Runtime, Value, Array};
use std::collections::HashMap;
use tracing::debug;

/// JavaScript interpreter for YouTube signature decryption
/// This uses rquickjs to execute the actual JavaScript signature functions
pub struct JSInterpreter {
    js_code: String,
}

impl JSInterpreter {
    pub fn new(js_code: String) -> Result<Self> {
        // Clean the JavaScript code to remove null bytes and other problematic characters
        let cleaned_js = Self::clean_js_code(&js_code);
        Ok(Self {
            js_code: cleaned_js,
        })
    }
    
    /// Clean JavaScript code by removing null bytes and other problematic characters
    fn clean_js_code(js_code: &str) -> String {
        // Only remove null bytes, but preserve all other characters including Unicode
        // YouTube's JS is heavily obfuscated and compressed, so we need to be very conservative
        js_code.replace('\0', "")
    }
    
    /// Extract and execute a signature function with the given signature
    pub fn decrypt_signature(&self, function_name: &str, signature: &str, globals: Option<HashMap<String, Vec<String>>>) -> Result<String> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        
        context.with(|ctx| {
            // Load the JavaScript code
            let _: Value = ctx.eval(self.js_code.as_bytes()).map_err(|e| {
                debug!("Failed to load JavaScript code: {}", e);
                debug!("JavaScript code length: {}", self.js_code.len());
                debug!("JavaScript code first 500 chars: {}", &self.js_code[..std::cmp::min(500, self.js_code.len())]);
                anyhow::anyhow!("Failed to evaluate JavaScript: {}", e)
            })?;
            debug!("JavaScript code loaded successfully");
            
            // Debug: Check what's available in the global context
            if let Some(global_obj) = ctx.globals().as_object() {
                let keys: Vec<String> = global_obj.keys::<String>()
                    .filter_map(|k| k.ok())
                    .take(20) // Limit to first 20 to avoid spam
                    .collect();
                debug!("Available globals after loading JS: {:?}", keys);
            }
            
            // Set up global variables if provided
            if let Some(globals) = globals {
                let globals_len = globals.len();
                for (name, array) in globals {
                    let js_array = Array::new(ctx.clone())?;
                    for (i, item) in array.iter().enumerate() {
                        js_array.set(i, item.clone())?;
                    }
                    ctx.globals().set(name, js_array)?;
                }
                debug!("Set up {} global variables", globals_len);
            }
            
            // Extract the function
            let func: rquickjs::Function = match ctx.globals().get(function_name) {
                Ok(f) => f,
                Err(e) => {
                    debug!("Failed to get function '{}': {}", function_name, e);
                    // Try to list available functions for debugging
                    if let Some(global_obj) = ctx.globals().as_object() {
                        let keys: Vec<String> = global_obj.keys::<String>()
                            .filter_map(|k| k.ok())
                            .collect();
                        debug!("Available global functions/variables: {:?}", keys);
                    }
                    return Err(anyhow::anyhow!("Function '{}' not found in JavaScript context", function_name));
                }
            };
            
            // Execute the function with the signature
            let result: String = func.call((signature,)).map_err(|e| {
                debug!("Failed to execute function '{}' with signature '{}': {}", function_name, signature, e);
                anyhow::anyhow!("Function execution failed: {}", e)
            })?;
            
            debug!("Signature decryption: {} -> {}", signature, result);
            Ok(result)
        })
    }
    
    /// Extract function code and arguments from JavaScript
    pub fn extract_function_code(&self, function_name: &str) -> Result<(Vec<String>, String)> {
        // Use regex to find the function definition
        let func_pattern = format!(r"function\s+{}\s*\([^)]*\)\s*\{{[^}}]*\}}", regex::escape(function_name));
        let re = regex::Regex::new(&func_pattern)?;
        
        if let Some(captures) = re.find(&self.js_code) {
            let func_code = captures.as_str();
            
            // Extract argument names
            let args_pattern = format!(r"function\s+{}\s*\(([^)]*)\)", regex::escape(function_name));
            let args_re = regex::Regex::new(&args_pattern)?;
            
            let args = if let Some(args_match) = args_re.captures(func_code) {
                args_match.get(1).unwrap().as_str()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                vec![]
            };
            
            Ok((args, func_code.to_string()))
        } else {
            anyhow::bail!("Could not find function {}", function_name)
        }
    }
    
    /// Extract global variables from JavaScript code
    pub fn extract_global_vars(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut globals = HashMap::new();
        
        // Look for global array patterns like: var a = ["string1", "string2", ...]
        let global_array_pattern = r#"var\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*=\s*\[((?:[^"\[\]]*"[^"]*"[^"\[\]]*,?\s*)*)\]"#;
        let re = regex::Regex::new(global_array_pattern)?;
        
        for captures in re.captures_iter(&self.js_code) {
            if let (Some(var_name), Some(array_content)) = (captures.get(1), captures.get(2)) {
                let var_name = var_name.as_str().to_string();
                
                // Extract string literals from array
                let string_pattern = r#""([^"]*)""#;
                let string_re = regex::Regex::new(string_pattern)?;
                
                let strings: Vec<String> = string_re.captures_iter(array_content.as_str())
                    .filter_map(|m| m.get(1).map(|s| s.as_str().to_string()))
                    .collect();
                
                if !strings.is_empty() {
                    globals.insert(var_name, strings);
                }
            }
        }
        
        debug!("Extracted global variables: {:?}", globals.keys().collect::<Vec<_>>());
        Ok(globals)
    }
    
    /// Execute JavaScript code and return the result
    pub fn execute(&self, code: &str) -> Result<String> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        
        context.with(|ctx| {
            let result: Value = ctx.eval(code.as_bytes())?;
            match result.as_string() {
                Some(s) => Ok(s.to_string()?),
                None => Ok(String::new()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_js_interpreter_basic() {
        let js_code = r#"
        function test(a) {
            return a + "world";
        }
        "#.to_string();
        
        let interpreter = JSInterpreter::new(js_code).unwrap();
        let result = interpreter.decrypt_signature("test", "hello", None).unwrap();
        assert_eq!(result, "helloworld");
    }
    
    #[test]
    fn test_signature_transformation() {
        let js_code = r#"
        var a = ["reverse", "splice", "swap"];
        function sig(s) {
            s = s.split('');
            s.reverse();
            s.splice(1, 1);
            var c = s[0];
            s[0] = s[2];
            s[2] = c;
            return s.join('');
        }
        "#.to_string();
        
        let interpreter = JSInterpreter::new(js_code).unwrap();
        let result = interpreter.decrypt_signature("sig", "abcdef", None).unwrap();
        // Original: "abcdef"
        // Reverse: "fedcba"  
        // Splice at 1: "fdcba"
        // Swap 0 and 2: "cdcba"
        assert_eq!(result, "cdcba");
    }
}