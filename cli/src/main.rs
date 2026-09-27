//! `reclass`: a command line client for a running reclass-server, for scripts
//! and coding agents. Every command is one `POST /api/rpc` call (plus a
//! definitions lookup to resolve names), so edits show up live in the web UI.

mod client;
mod defs;
mod render;

use anyhow::{
    bail,
    Result,
};
use clap::{
    Parser,
    Subcommand,
};
use client::Client;
use defs::{
    number,
    Defs,
};
use serde_json::{
    json,
    Value,
};

const ABOUT: &str = "Explore and annotate process memory through a running reclass-server.";

const LONG_ABOUT: &str = "\
Explore and annotate process memory through a running reclass-server.

Start the server first (`cargo run --release -p reclass-server -- --demo` for a
simulated process) and export the API token it prints as RECLASS_API_TOKEN.
The browser UI shows every change live.";

const GUIDE: &str = "\
Addresses (EXPR) are expressions:
  0x7FF6A000 or 1234      hex needs 0x; bare digits are decimal
  <game.exe>              module base
  $GWorld                 signature value (see `reclass sigs`)
  [EXPR]                  read a u64 at EXPR
  + - ( )                 arithmetic, e.g. \"[<game.exe> + 0x5A1230] + 0x10\"

Classes are named, or #ID. Fields are Class.name, or Class+OFFSET for any field
including unnamed hex fields, e.g. Player.health or Player+0x1C.

Types (TYPE):
  hex8..hex64  i8..i64  u8..u64  bool  f32  f64  vec2  vec3  vec4
  text (inline 32 chars)  char* (text pointer)  ptr (untyped pointer)
  Player        embedded class instance     ETeam      enum
  Player*       pointer to a class          u32*       pointer to a primitive
  f32[4]        inline array                Player*[8] array of pointers
  enc Player*   encrypted pointer
Changing a field's size keeps later fields in place: a smaller type is padded
with hex fields, a larger one consumes the fields after it.

Typical loop:
  reclass status                          what is attached, where the root is
  reclass view                            the root class, decoded live
  reclass view 0x1F3A8D12600 -s 0x100     raw qwords with pointer/float hints
  reclass define Player 0x1C f32 health   name what you found
  reclass view '[$GWorld]+0x28' -c Player -f 1   follow pointers one level

Pointers are printed with 0x so they can be pasted back as EXPR.
Add --json to any command for the server's raw reply.";

#[derive(Parser)]
#[command(name = "reclass", about = ABOUT, long_about = LONG_ABOUT, after_long_help = GUIDE, after_help = "Run `reclass --help` for expression and type syntax.")]
struct Cli {
    /// Server URL.
    #[arg(
        long,
        env = "RECLASS_URL",
        default_value = "http://127.0.0.1:7878",
        global = true
    )]
    url: String,
    /// API token printed by reclass-server. Prefer the environment variable; a flag is visible to other processes.
    #[arg(long, env = "RECLASS_API_TOKEN", hide_env_values = true, global = true)]
    token: Option<String>,
    /// Print the server's raw JSON reply instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Attached process, root address and project.
    Status,
    /// List processes, optionally filtered by name.
    Processes { filter: Option<String> },
    /// Attach to a process by pid or name.
    Attach { process: String },
    /// Detach from the process.
    Detach,
    /// List loaded modules, optionally filtered by name.
    Modules { filter: Option<String> },
    /// Evaluate an address expression.
    Eval { expr: String },
    /// Hex dump of memory.
    Read {
        expr: String,
        /// Bytes to read (max 0x10000).
        #[arg(default_value = "0x100")]
        len: String,
    },
    /// Decode memory live. With no address, the root class at the root address.
    #[command(after_help = "Without -c or -s, an address shows 0x80 bytes of hex fields.")]
    View {
        /// Address expression.
        expr: Option<String>,
        /// Decode as this class.
        #[arg(short, long, conflicts_with = "size")]
        class: Option<String>,
        /// Decode this many bytes as hex fields instead of a class.
        #[arg(short, long)]
        size: Option<String>,
        /// Follow class pointers this many levels deep (max 8).
        #[arg(short, long, default_value_t = 0)]
        follow: u32,
        /// Array elements to show per array.
        #[arg(short, long, default_value_t = 16)]
        elements: u32,
    },
    /// List classes, optionally filtered by name.
    Classes { filter: Option<String> },
    /// Show a class definition: offsets, sizes, types and names.
    Class { class: String },
    /// List enums.
    Enums,
    /// List signatures and their resolved values.
    Sigs,
    /// Show the root, or set its address expression and/or class.
    Root {
        expr: Option<String>,
        #[arg(short, long)]
        class: Option<String>,
    },
    /// Give the bytes at an offset a type (and name), splitting hex fields as needed.
    Define {
        class: String,
        offset: String,
        #[arg(value_name = "TYPE")]
        ty: String,
        name: Option<String>,
    },
    /// Change a field's type.
    Retype {
        /// Class.name or Class+OFFSET
        field: String,
        #[arg(value_name = "TYPE")]
        ty: String,
    },
    /// Rename a field (Class.name or Class+OFFSET) or a class.
    Rename { target: String, name: String },
    /// Create a class of hex fields.
    NewClass {
        name: String,
        /// Size in bytes.
        #[arg(short, long, default_value = "0x40")]
        size: String,
    },
    /// Delete a class that nothing refers to.
    DeleteClass { class: String },
    /// Insert hex bytes into a class, at the end unless --before or --after is given.
    Insert {
        class: String,
        count: String,
        #[arg(long, conflicts_with = "after")]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
    },
    /// Remove a field; later fields move up.
    Remove { field: String },
    /// Create or update an enum. Variants replace the existing ones.
    Enum {
        name: String,
        /// Variants as Name=Value.
        variants: Vec<String>,
        /// Size in bytes: 1, 2, 4 or 8.
        #[arg(long)]
        size: Option<u8>,
        #[arg(long)]
        flags: bool,
    },
    /// Delete an enum that nothing refers to.
    DeleteEnum { name: String },
    /// Create or replace a signature, usable as $NAME in expressions.
    #[command(after_help = "\
With --relative, the value is the address that a relative operand at
match+OFFSET points to (e.g. `mov rax, [rip+x]` or `jmp rel32`). Without it,
the value is the u32 at match+OFFSET, such as a struct offset encoded in an
instruction; it is not the address of the match.")]
    Sig {
        name: String,
        #[arg(long)]
        module: String,
        /// IDA-style pattern, e.g. "48 8B 05 ? ? ? ?".
        #[arg(long)]
        pattern: String,
        #[arg(long, default_value = "0")]
        offset: String,
        /// Resolve a RIP-relative operand; the value is the instruction length.
        #[arg(long, value_name = "INST_LEN")]
        relative: Option<String>,
    },
    /// Delete a signature.
    DeleteSig { name: String },
    /// Save the project, to PATH or the current project file.
    Save { path: Option<String> },
    /// Send any server method, e.g. `reclass call snapshot`.
    Call {
        method: String,
        /// JSON params.
        params: Option<String>,
    },
}

struct App {
    client: Client,
    json: bool,
}

impl App {
    fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.client.call(method, params)
    }

    fn state(&self) -> Result<Value> {
        self.call("state", Value::Null)
    }

    fn defs(&self) -> Result<Defs> {
        Ok(Defs::from_state(&self.state()?))
    }

    /// One line describing a field after an edit, so the result can be checked.
    fn show_field(&self, class_id: &Value, field_id: &Value) -> Result<String> {
        let defs = self.defs()?;
        let class = defs.class(&format!("#{class_id}"))?;
        let f = class["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["id"] == *field_id);
        Ok(match f {
            Some(f) => {
                let line = format!(
                    "{}+0x{:X}  {}  {}",
                    class["name"].as_str().unwrap_or("?"),
                    f["offset"].as_u64().unwrap_or(0),
                    f["typeLabel"].as_str().unwrap_or("?"),
                    f["name"].as_str().unwrap_or("")
                );
                format!("{}\n", line.trim_end())
            }
            None => "ok\n".into(),
        })
    }

    fn run(&self, command: Command) -> Result<String> {
        use Command::*;
        let raw = |v: Value| Ok(format!("{}\n", serde_json::to_string_pretty(&v)?));
        match command {
            Status => {
                let s = self.state()?;
                if self.json {
                    return raw(s);
                }
                let session = &s["session"];
                let root = self.call("eval", json!({ "expr": session["rootExpr"] }));
                let defs = Defs::from_state(&s);
                let root_class = defs
                    .class(&format!("#{}", session["rootClassId"]))
                    .map(|c| c["name"].as_str().unwrap_or("?").to_string())
                    .unwrap_or_else(|_| "?".into());
                let process = match &session["attached"] {
                    Value::Null => "not attached (reclass processes / reclass attach)".into(),
                    p => format!("{} (pid {})", p["name"].as_str().unwrap_or("?"), p["pid"]),
                };
                let root = match root {
                    Ok(a) => format!("0x{}", a.as_str().unwrap_or("?")),
                    Err(e) => format!("does not resolve: {e}"),
                };
                let project = match session["projectPath"].as_str() {
                    Some(p) => p.to_string(),
                    None => "unsaved new project".into(),
                };
                Ok(render::table(&[
                    vec!["process".into(), process],
                    vec![
                        "root".into(),
                        format!(
                            "{root_class} @ {} = {root}",
                            session["rootExpr"].as_str().unwrap_or("")
                        ),
                    ],
                    vec![
                        "project".into(),
                        format!(
                            "{project}{}",
                            if session["dirty"] == true {
                                " (unsaved changes)"
                            } else {
                                ""
                            }
                        ),
                    ],
                    vec![
                        "defs".into(),
                        format!("{} classes, {} enums", defs.classes.len(), defs.enums.len()),
                    ],
                ]))
            }
            Processes { filter } => {
                let list = self.call("processes", Value::Null)?;
                let list = filtered(&list, "name", filter.as_deref());
                if self.json {
                    return raw(json!(list));
                }
                let rows: Vec<Vec<String>> = list
                    .iter()
                    .map(|p| {
                        vec![
                            p["pid"].to_string(),
                            p["name"].as_str().unwrap_or("").into(),
                        ]
                    })
                    .collect();
                Ok(render::table(&rows))
            }
            Attach { process } => {
                let pid = match number(&process) {
                    Ok(pid) => pid,
                    Err(_) => self.find_process(&process)?,
                };
                let r = self.call("attach", json!({ "pid": pid }))?;
                if self.json {
                    return raw(r);
                }
                Ok(format!("attached to {pid}\n"))
            }
            Detach => self
                .call("detach", Value::Null)
                .map(|_| "detached\n".into()),
            Modules { filter } => {
                let list = self.call("modules", Value::Null)?;
                let list = filtered(&list, "name", filter.as_deref());
                if self.json {
                    return raw(json!(list));
                }
                let rows: Vec<Vec<String>> = list
                    .iter()
                    .map(|m| {
                        vec![
                            format!("0x{}", m["base"].as_str().unwrap_or("")),
                            format!("0x{:X}", m["size"].as_u64().unwrap_or(0)),
                            m["name"].as_str().unwrap_or("").into(),
                        ]
                    })
                    .collect();
                Ok(render::table(&rows))
            }
            Eval { expr } => {
                let r = self.call("eval", json!({ "expr": expr }))?;
                if self.json {
                    return raw(r);
                }
                Ok(format!("0x{}\n", r.as_str().unwrap_or("")))
            }
            Read { expr, len } => {
                let r = self.call("read", json!({ "address": expr, "len": number(&len)? }))?;
                if self.json {
                    return raw(r);
                }
                let address = u64::from_str_radix(r["address"].as_str().unwrap_or("0"), 16)?;
                Ok(render::hexdump(address, r["bytes"].as_str().unwrap_or("")))
            }
            View {
                expr,
                class,
                size,
                follow,
                elements,
            } => {
                let mut params = json!({ "follow": follow, "elements": elements });
                if let Some(e) = &expr {
                    params["address"] = json!(e);
                }
                match (class, size) {
                    (Some(c), _) => params["classId"] = self.defs()?.class(&c)?["id"].clone(),
                    (None, Some(s)) => params["size"] = json!(number(&s)?),
                    (None, None) if expr.is_some() => params["size"] = json!(0x80),
                    (None, None) => {}
                }
                let r = self.call("inspect", params)?;
                if self.json {
                    return raw(r);
                }
                Ok(render::instance(&r))
            }
            Classes { filter } => {
                let s = self.state()?;
                let defs = Defs::from_state(&s);
                let list = filtered(&json!(defs.classes), "name", filter.as_deref());
                if self.json {
                    return raw(json!(list));
                }
                let root = &s["session"]["rootClassId"];
                let rows: Vec<Vec<String>> = list
                    .iter()
                    .map(|c| {
                        vec![
                            format!("#{}", c["id"]),
                            c["name"].as_str().unwrap_or("").into(),
                            format!("0x{:X}", c["size"].as_u64().unwrap_or(0)),
                            format!(
                                "{} refs{}",
                                c["refs"],
                                if c["id"] == *root { ", root" } else { "" }
                            ),
                        ]
                    })
                    .collect();
                Ok(render::table(&rows))
            }
            Class { class } => {
                let s = self.state()?;
                let defs = Defs::from_state(&s);
                let c = defs.class(&class)?;
                if self.json {
                    return raw(c.clone());
                }
                Ok(render::layout(c, c["id"] == s["session"]["rootClassId"]))
            }
            Enums => {
                let defs = self.defs()?;
                if self.json {
                    return raw(json!(defs.enums));
                }
                let rows: Vec<Vec<String>> = defs
                    .enums
                    .iter()
                    .map(|e| {
                        let variants: Vec<String> = e["variants"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|v| format!("{}={}", v[0].as_str().unwrap_or(""), v[1]))
                            .collect();
                        vec![
                            format!("#{}", e["id"]),
                            e["name"].as_str().unwrap_or("").into(),
                            format!(
                                "size {}{}",
                                e["size"],
                                if e["isFlags"] == true { ", flags" } else { "" }
                            ),
                            variants.join(" "),
                        ]
                    })
                    .collect();
                Ok(render::table(&rows))
            }
            Sigs => {
                let s = self.state()?;
                let sigs = &s["defs"]["signatures"];
                if self.json {
                    return raw(sigs.clone());
                }
                let rows: Vec<Vec<String>> = sigs
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|g| {
                        let value = match (g["value"].as_str(), g["error"].as_str()) {
                            (Some(v), _) => format!("0x{v}"),
                            (None, Some(e)) => format!("!! {e}"),
                            _ => "?".into(),
                        };
                        let mut how = format!("offset {}", g["offset"]);
                        if g["isRelative"] == true {
                            how.push_str(&format!(", relative inst len {}", g["relInstLen"]));
                        }
                        vec![
                            format!("${}", g["name"].as_str().unwrap_or("")),
                            value,
                            g["module"].as_str().unwrap_or("").into(),
                            how,
                            g["pattern"].as_str().unwrap_or("").into(),
                        ]
                    })
                    .collect();
                Ok(render::table(&rows))
            }
            Root { expr, class } => {
                if expr.is_none() && class.is_none() {
                    return self.run(Status);
                }
                let mut params = json!({});
                if let Some(e) = expr {
                    params["expr"] = json!(e);
                }
                if let Some(c) = class {
                    params["classId"] = self.defs()?.class(&c)?["id"].clone();
                }
                self.call("setRoot", params)?;
                self.run(Status)
            }
            Define {
                class,
                offset,
                ty,
                name,
            } => {
                let defs = self.defs()?;
                let class_id = defs.class(&class)?["id"].clone();
                let mut params = defs.type_spec(&ty)?;
                params["classId"] = class_id.clone();
                params["offset"] = json!(number(&offset)?);
                let field_id = self.call("defineAt", params)?;
                if let Some(name) = name {
                    self.call(
                        "renameField",
                        json!({ "classId": class_id, "fieldId": field_id, "name": name }),
                    )?;
                }
                self.show_field(&class_id, &field_id)
            }
            Retype { field, ty } => {
                let defs = self.defs()?;
                let f = defs.field(&field)?;
                let mut params = defs.type_spec(&ty)?;
                params["classId"] = f.class["id"].clone();
                params["fieldId"] = f.field["id"].clone();
                self.call("retype", params)?;
                self.show_field(&f.class["id"], &f.field["id"])
            }
            Rename { target, name } => {
                let defs = self.defs()?;
                if target.contains(['.', '+']) {
                    let f = defs.field(&target)?;
                    let mut params = f.params();
                    params["name"] = json!(name);
                    self.call("renameField", params)?;
                    self.show_field(&f.class["id"], &f.field["id"])
                } else {
                    let id = defs.class(&target)?["id"].clone();
                    self.call("renameClass", json!({ "classId": id, "name": name }))?;
                    Ok(format!("renamed class #{id} to {name}\n"))
                }
            }
            NewClass { name, size } => {
                let size = number(&size)?;
                if size == 0 || size > 0x10000 {
                    bail!("class size must be 1 to 0x10000 bytes");
                }
                let id = self.call("addClass", json!({ "name": name }))?;
                self.resize(&id, size)?;
                let s = self.state()?;
                let defs = Defs::from_state(&s);
                Ok(render::layout(defs.class(&format!("#{id}"))?, false))
            }
            DeleteClass { class } => {
                let id = self.defs()?.class(&class)?["id"].clone();
                self.call("deleteClass", json!({ "classId": id }))?;
                Ok(format!("deleted class #{id}\n"))
            }
            Insert {
                class,
                count,
                before,
                after,
            } => {
                let defs = self.defs()?;
                let class = defs.class(&class)?;
                let mut params = json!({ "classId": class["id"], "count": number(&count)? });
                if let Some(at) = before.as_ref().or(after.as_ref()) {
                    let f = defs.field(&qualify(class, at))?;
                    params["fieldId"] = f.field["id"].clone();
                    params["after"] = json!(after.is_some());
                }
                self.call("insertBytes", params)?;
                let s = self.state()?;
                Ok(render::layout(
                    Defs::from_state(&s).class(&format!("#{}", class["id"]))?,
                    false,
                ))
            }
            Remove { field } => {
                let defs = self.defs()?;
                let f = defs.field(&field)?;
                self.call("removeField", f.params())?;
                Ok(format!("removed {field}\n"))
            }
            Enum {
                name,
                variants,
                size,
                flags,
            } => {
                let defs = self.defs()?;
                let existing = defs.enumeration(&name).ok().cloned();
                let id = match &existing {
                    Some(e) => e["id"].clone(),
                    None => self.call("addEnum", json!({ "name": name, "size": size }))?,
                };
                let variants: Vec<Value> = if variants.is_empty() {
                    existing
                        .as_ref()
                        .and_then(|e| e["variants"].as_array().cloned())
                        .unwrap_or_default()
                } else {
                    variants
                        .iter()
                        .map(|v| {
                            let (n, val) = v.split_once('=').ok_or_else(|| {
                                anyhow::anyhow!("variant '{v}' is not Name=Value")
                            })?;
                            Ok(json!([n.trim(), number(val)?]))
                        })
                        .collect::<Result<_>>()?
                };
                let size = size
                    .map(Value::from)
                    .or_else(|| existing.as_ref().map(|e| e["size"].clone()))
                    .unwrap_or(json!(4));
                let flags = flags || existing.is_some_and(|e| e["isFlags"] == true);
                self.call(
                    "updateEnum",
                    json!({ "enumId": id, "name": name, "isFlags": flags, "size": size, "variants": variants }),
                )?;
                Ok(format!("enum {name} #{id}\n"))
            }
            DeleteEnum { name } => {
                let id = self.defs()?.enumeration(&name)?["id"].clone();
                self.call("deleteEnum", json!({ "enumId": id }))?;
                Ok(format!("deleted enum #{id}\n"))
            }
            Sig {
                name,
                module,
                pattern,
                offset,
                relative,
            } => {
                let signature = json!({
                    "name": name,
                    "module": module,
                    "pattern": pattern,
                    "offset": number(&offset)?,
                    "is_relative": relative.is_some(),
                    "rel_inst_len": relative.as_deref().map(number).transpose()?.unwrap_or(0),
                });
                self.call("setSignature", json!({ "signature": signature }))?;
                let s = self.state()?;
                let g = s["defs"]["signatures"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|g| g["name"] == name);
                Ok(match g.map(|g| (&g["value"], &g["error"])) {
                    Some((Value::String(v), _)) => format!("${name} = 0x{v}\n"),
                    Some((_, Value::String(e))) => {
                        format!("${name} saved but does not resolve: {e}\n")
                    }
                    _ => format!("${name} saved\n"),
                })
            }
            DeleteSig { name } => {
                self.call("removeSignature", json!({ "name": name }))?;
                Ok(format!("deleted ${name}\n"))
            }
            Save { path } => {
                let params = match path {
                    Some(p) => json!({ "path": p }),
                    None => json!({}),
                };
                let r = self.call("save", params)?;
                Ok(format!("saved {}\n", r.as_str().unwrap_or("")))
            }
            Call { method, params } => {
                let params = match params {
                    Some(p) => serde_json::from_str(&p)?,
                    None => Value::Null,
                };
                raw(self.call(&method, params)?)
            }
        }
    }

    fn find_process(&self, name: &str) -> Result<u64> {
        let list = self.call("processes", Value::Null)?;
        let all = list.as_array().map(Vec::as_slice).unwrap_or(&[]);
        let exact: Vec<&Value> = all
            .iter()
            .filter(|p| {
                p["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .collect();
        let hits = if exact.is_empty() {
            filtered(&list, "name", Some(name))
        } else {
            exact.into_iter().cloned().collect()
        };
        match hits.as_slice() {
            [p] => Ok(p["pid"].as_u64().unwrap_or(0)),
            [] => bail!("no process matches '{name}'"),
            many => {
                let names: Vec<String> = many
                    .iter()
                    .map(|p| format!("{} ({})", p["name"].as_str().unwrap_or(""), p["pid"]))
                    .collect();
                bail!(
                    "'{name}' matches several processes; pass a pid: {}",
                    names.join(", ")
                )
            }
        }
    }

    /// Replaces the fields of a new class with `size` bytes of hex fields.
    fn resize(&self, class_id: &Value, size: u64) -> Result<()> {
        if size == 0 {
            bail!("a class needs at least one byte");
        }
        let defs = self.defs()?;
        let class = defs.class(&format!("#{class_id}"))?;
        if class["size"] == size {
            return Ok(());
        }
        self.call("insertBytes", json!({ "classId": class_id, "count": size }))?;
        for f in class["fields"].as_array().into_iter().flatten() {
            self.call(
                "removeField",
                json!({ "classId": class_id, "fieldId": f["id"] }),
            )?;
        }
        Ok(())
    }
}

/// Entries whose `key` contains `filter`, case-insensitively.
fn filtered(list: &Value, key: &str, filter: Option<&str>) -> Vec<Value> {
    let filter = filter.map(str::to_ascii_lowercase);
    list.as_array()
        .into_iter()
        .flatten()
        .filter(|v| match &filter {
            Some(f) => v[key]
                .as_str()
                .is_some_and(|n| n.to_ascii_lowercase().contains(f)),
            None => true,
        })
        .cloned()
        .collect()
}

/// Lets `--before health` mean `--before Player.health`.
fn qualify(class: &Value, field: &str) -> String {
    if field.contains(['.', '+']) {
        field.to_string()
    } else if field.starts_with("0x") || field.bytes().all(|b| b.is_ascii_digit()) {
        format!("{}+{field}", class["name"].as_str().unwrap_or(""))
    } else {
        format!("{}.{field}", class["name"].as_str().unwrap_or(""))
    }
}

fn main() {
    let cli = Cli::parse();
    let app = App {
        client: Client::new(&cli.url, cli.token),
        json: cli.json,
    };
    match app.run(cli.command) {
        Ok(out) => print!("{out}"),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}
