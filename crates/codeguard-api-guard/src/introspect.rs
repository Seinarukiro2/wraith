use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

const INTROSPECT_SCRIPT: &str = r#"
import sys, json, importlib, inspect, warnings

def introspect(module_name, attr_name):
    result = {
        "exists": False,
        "module_found": False,
        "kind": None,
        "signature": None,
        "deprecated": False,
        "all_attributes": [],
        "closest_match": None,
    }

    try:
        mod = importlib.import_module(module_name)
    except (ImportError, ModuleNotFoundError):
        return result

    result["module_found"] = True

    result["all_attributes"] = [a for a in dir(mod) if not a.startswith("_")]

    if not hasattr(mod, attr_name):
        # Find closest match
        from difflib import get_close_matches
        matches = get_close_matches(attr_name, result["all_attributes"], n=1, cutoff=0.6)
        if matches:
            result["closest_match"] = matches[0]
        return result

    result["exists"] = True
    obj = getattr(mod, attr_name)

    if callable(obj):
        result["kind"] = "function"
        try:
            sig = inspect.signature(obj)
            params = []
            has_var_keyword = False
            for name, param in sig.parameters.items():
                kind = str(param.kind)
                if "VAR_KEYWORD" in kind:
                    has_var_keyword = True
                params.append({
                    "name": name,
                    "kind": kind,
                    "has_default": param.default is not inspect.Parameter.empty,
                })
            result["signature"] = {
                "params": params,
                "has_var_keyword": has_var_keyword,
            }
        except (ValueError, TypeError):
            pass

        # Check deprecation
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always", DeprecationWarning)
            try:
                doc = inspect.getdoc(obj) or ""
                if "deprecated" in doc.lower():
                    result["deprecated"] = True
            except Exception:
                pass
    else:
        result["kind"] = "attribute"

    return result

data = json.loads(sys.stdin.read())
results = {}
for q in data.get("queries", []):
    key = f"{q['module']}.{q['attribute']}"
    results[key] = introspect(q["module"], q["attribute"])

json.dump(results, sys.stdout)
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectResult {
    pub exists: bool,
    #[serde(default)]
    pub module_found: bool,
    pub kind: Option<String>,
    pub signature: Option<SignatureInfo>,
    pub deprecated: bool,
    pub all_attributes: Vec<String>,
    pub closest_match: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfo {
    pub params: Vec<ParamInfo>,
    pub has_var_keyword: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: String,
    pub kind: String,
    pub has_default: bool,
}

pub struct PythonIntrospector {
    python_exec: String,
}

impl PythonIntrospector {
    pub fn new(python_exec: String) -> Self {
        Self { python_exec }
    }

    pub fn batch_introspect(
        &self,
        queries: &[(String, String)],
    ) -> Result<HashMap<String, IntrospectResult>> {
        if queries.is_empty() {
            return Ok(HashMap::new());
        }

        let input = serde_json::json!({
            "queries": queries.iter().map(|(m, a)| {
                serde_json::json!({"module": m, "attribute": a})
            }).collect::<Vec<_>>()
        });

        let mut child = Command::new(&self.python_exec)
            .args(["-c", INTROSPECT_SCRIPT])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        {
            let stdin = child.stdin.as_mut().unwrap();
            stdin.write_all(input.to_string().as_bytes())?;
        }

        let output = child.wait_with_output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Python introspection failed: {stderr}");
        }

        let stdout = String::from_utf8(output.stdout)?;
        let results: HashMap<String, IntrospectResult> = serde_json::from_str(&stdout)?;
        Ok(results)
    }
}
