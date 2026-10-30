#!/usr/bin/env python3
"""Writes crates/uwulock-ffi/src/dispatch.rs from apps/desktop/src-tauri/src/commands.rs: the
native Apple app's command table, the same commands with the same arguments as the Tauri app.

    python3 scripts/ffi-dispatch.py . && cargo fmt -p uwulock-ffi
"""
import re
import sys

root = sys.argv[1] if len(sys.argv) > 1 else "."
src = open(f"{root}/apps/desktop/src-tauri/src/commands.rs").read()

mod_re = re.compile(r"^pub\(crate\) mod (\w+) \{\n(.*?)^\}\n", re.S | re.M)
fn_re = re.compile(
    r"#\[tauri::command\]\n(?:\s*#\[allow\(clippy::too_many_arguments\)\]\n)?"
    r"\s*pub\(crate\) (async )?fn (\w+)\((.*?)\)\s*(?:->\s*(.*?))?\s*\{\n(.*?)\n    \}\n",
    re.S,
)


def split_params(text):
    out, depth, cur = [], 0, ""
    for ch in text:
        if ch in "<(":
            depth += 1
        elif ch in ">)":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur)
    return [p.strip() for p in out if p.strip()]


def camel(name):
    head, *rest = name.split("_")
    return head + "".join(p.capitalize() for p in rest)


def pascal(name):
    return "".join(p.capitalize() for p in name.split("_"))


modules = []
commands = []
for m in mod_re.finditer(src):
    mod, body = m.group(1), m.group(2)
    uses = []
    for line in body.splitlines():
        s = line.strip()
        if s.startswith("use super::"):
            continue
        if s.startswith("use ") or s.startswith("#[allow(unused_imports)]"):
            uses.append(s)
    # drop an allow attribute left without its use
    clean = []
    for i, s in enumerate(uses):
        if s.startswith("#[allow") and (i + 1 >= len(uses) or not uses[i + 1].startswith("use ")):
            continue
        clean.append(s)
    modules.append((mod, clean))
    for f in fn_re.finditer(body):
        is_async, name, params, ret, fbody = f.groups()
        args = []
        for p in split_params(params):
            pname, ptype = p.split(":", 1)
            pname, ptype = pname.strip(), " ".join(ptype.split())
            if ptype in ("AppHandle", "State<'_, App>") or ptype.startswith("tauri::ipc::Request"):
                continue
            args.append((pname, ptype))
        ret = " ".join((ret or "()").split())
        commands.append(dict(mod=mod, name=name, is_async=bool(is_async), args=args, ret=ret))

assert len(commands) == 121, len(commands)


def call_expr(c):
    a = ", ".join(f"a.{n}" for n, _ in c["args"])
    if c["name"] == "stage_send_file":
        a = "None"
    e = f"app.{c['name']}({a})"
    if c["is_async"]:
        e += ".await"
    r = c["ret"]
    if r.startswith("Result<"):
        return f"json({e}?)"
    if r.startswith("std::result::Result<"):
        return f"json({e}.map_err(|message| Failure::new(\"unknown\", message))?)"
    if r == "()":
        return f"{{\n                {e};\n                json(())\n            }}"
    return f"json({e})"


def variant(c):
    v = pascal(c["name"])
    return f"{v}({c['mod']}::{v})" if c["args"] else v


def pattern(c):
    v = pascal(c["name"])
    return f"{v}(a)" if c["args"] else v


out = []
w = out.append
w("//! The command table: every IPC command of the Tauri app (apps/desktop/src-tauri/src/commands.rs)")
w("//! with the same name and the same camelCase arguments. Written by scripts/ffi-dispatch.py from")
w("//! that file: run it again when a command there changes.")
w("//!")
w("//! [`parse`] reads the arguments without running anything; [`run_now`] runs what Tauri runs on")
w("//! its own thread (quick, synchronous), [`run_later`] what it awaits.")
w("")
w("use serde::Serialize;")
w("use uwulock_app::{App, Failure};")
w("")
w("/// A command with its arguments read.")
w("#[allow(clippy::large_enum_variant)]")
w("pub(crate) enum Call {")
w("    Now(Now),")
w("    Later(Later),")
w("}")
w("")
w("/// The synchronous commands. Each lives for one call: no boxing.")
w("#[allow(clippy::large_enum_variant)]")
w("pub(crate) enum Now {")
for c in commands:
    if not c["is_async"]:
        w(f"    {variant(c)},")
w("}")
w("")
w("/// The asynchronous commands.")
w("#[allow(clippy::large_enum_variant)]")
w("pub(crate) enum Later {")
for c in commands:
    if c["is_async"]:
        w(f"    {variant(c)},")
w("}")
w("")
w("/// Every command's name, in the order of commands.rs.")
w("pub(crate) const COMMANDS: &[&str] = &[")
for c in commands:
    w(f"    \"{c['name']}\",")
w("];")
w("")
w("fn args<T: serde::de::DeserializeOwned>(command: &str, json: &str) -> Result<T, Failure> {")
w("    let json = if json.trim().is_empty() { \"{}\" } else { json };")
w("    serde_json::from_str(json).map_err(|error| {")
w("        Failure::new(\"args\", format!(\"{command}: {error}\"))")
w("    })")
w("}")
w("")
w("fn json<T: Serialize>(value: T) -> Result<String, Failure> {")
w("    serde_json::to_string(&value).map_err(|error| Failure::new(\"failed\", error.to_string()))")
w("}")
w("")
w("/// Reads `json` as the arguments of `command`.")
w("pub(crate) fn parse(command: &str, json: &str) -> Result<Call, Failure> {")
w("    Ok(match command {")
for c in commands:
    side = "Later" if c["is_async"] else "Now"
    v = pascal(c["name"])
    if c["args"]:
        w(f"        \"{c['name']}\" => Call::{side}({side}::{v}(args(command, json)?)),")
    else:
        w(f"        \"{c['name']}\" => Call::{side}({side}::{v}),")
w("        _ => {")
w("            return Err(Failure::new(")
w("                \"unknown-command\",")
w("                format!(\"No command {command}.\"),")
w("            ))")
w("        }")
w("    })")
w("}")
w("")
w("/// Runs a synchronous command; the answer as JSON.")
w("pub(crate) fn run_now(app: &App, call: Now) -> Result<String, Failure> {")
w("    match call {")
for c in commands:
    if not c["is_async"]:
        w(f"        Now::{pattern(c)} => {call_expr(c)},")
w("    }")
w("}")
w("")
w("/// Runs an asynchronous command; the answer as JSON.")
w("pub(crate) async fn run_later(app: App, call: Later) -> Result<String, Failure> {")
w("    match call {")
for c in commands:
    if c["is_async"]:
        w(f"        Later::{pattern(c)} => {call_expr(c)},")
w("    }")
w("}")

for mod, uses in modules:
    cs = [c for c in commands if c["mod"] == mod and c["args"]]
    w("")
    w("/// The arguments, with the types the Tauri command's module uses.")
    w("#[allow(unused_imports)]")
    w(f"pub(crate) mod {mod} {{")
    if cs:
        w("    use serde::Deserialize;")
        for u in uses:
            if not u.startswith("#["):
                w(f"    {u}")
    for c in cs:
        w("")
        w("    #[derive(Deserialize)]")
        w("    #[serde(rename_all = \"camelCase\")]")
        w(f"    pub(crate) struct {pascal(c['name'])} {{")
        for n, t in c["args"]:
            w(f"        pub(crate) {n}: {t},")
        w("    }")
    w("}")

# The table test's arguments: every required argument filled in.
samples = {
    "String": '"x"', "bool": "true", "u8": "1", "u32": "1", "u64": "1", "usize": "0",
    "Vec<u8>": "[1, 2]", "Vec<String>": '["x"]',
    "ServerInput": '{"kind": "bitwarden"}',
    "generator::Options": '{"length": 20, "lowercase": true, "uppercase": true, "digits": true, "symbols": false, "avoidAmbiguous": false}',
    "Draft": '{"kind": "login", "name": "x"}',
    "FileRequestInput": '{"label": "x", "title": "x", "maxFiles": 1, "textAllowed": true}',
    "ShareInput": '{"fields": [["password", "Password"]], "deletionDays": 7}',
    "ProblemKind": '"weak"',
    "Vec<Op>": '[]',
    "SendDraft": '{"kind": 0, "name": "x", "deletionDate": "2030-01-01T00:00:00Z"}',
    "Settings": '{}',
}
table = {}
for c in commands:
    fields = []
    for n, t in c["args"]:
        if t.startswith("Option<"):
            continue
        fields.append(f'"{camel(n)}": {samples[t]}')
    table[c["name"]] = "{" + ", ".join(fields) + "}"
w("")
w("#[cfg(test)]")
w("mod tests {")
w("    use super::*;")
w("")
w("    /// Every command with its required arguments, as the page sends them.")
w("    const SAMPLES: &[(&str, &str)] = &[")
for c in commands:
    w(f"        (\"{c['name']}\", r#\"{table[c['name']]}\"#),")
w("    ];")
w("")
w("    #[test]")
w("    fn every_command_reads_its_arguments() {")
w("        assert_eq!(SAMPLES.len(), COMMANDS.len());")
w("        for ((name, json), listed) in SAMPLES.iter().zip(COMMANDS) {")
w("            assert_eq!(name, listed);")
w("            if let Err(failure) = parse(name, json) {")
w("                panic!(\"{name}: {}\", failure.message());")
w("            }")
w("        }")
w("    }")
w("")
w("    #[test]")
w("    fn a_missing_argument_says_which() {")
w("        let Err(failure) = parse(\"login\", r#\"{\"server\": {\"kind\": \"bitwarden\"}}\"#) else {")
w("            panic!(\"login without an email\");")
w("        };")
w("        assert_eq!(failure.kind(), \"args\");")
w("        assert!(failure.message().contains(\"email\"), \"{}\", failure.message());")
w("    }")
w("")
w("    #[test]")
w("    fn an_unknown_command_is_an_error() {")
w("        let Err(failure) = parse(\"format_disk\", \"{}\") else {")
w("            panic!(\"an unknown command ran\");")
w("        };")
w("        assert_eq!(failure.kind(), \"unknown-command\");")
w("    }")
w("}")
open(f"{root}/crates/uwulock-ffi/src/dispatch.rs", "w").write("\n".join(out) + "\n")
print("ok", len(commands), sum(c["is_async"] for c in commands))
