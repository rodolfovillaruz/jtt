# jtt — JSON Talk Terminal

Pretty-print a JSON chat log as styled terminal bubbles, right-aligned for
`user`, left-aligned for `assistant`, and full-width for `system` messages.
Bubbles are capped at 75 % of the terminal width so long conversations remain
readable at a glance.

Output is sent through your `$PAGER` (falling back to `less -R`, then `more`,
then a built-in interactive pager) when stdout is a terminal, or piped
straight through when it is not.

```
╭─────────────────────────────────────────────╮          ╭────────────────────────╮
│ Assistant                                   │          │ You                    │
├─────────────────────────────────────────────┤          ├────────────────────────┤
│ Sure! Rust ownership means every value has  │          │ Can you explain Rust   │
│ exactly one owner at a time.  When the      │          │ ownership briefly?     │
│ owner goes out of scope the value is        │          ╰────────────────────────╯
│ dropped automatically — no GC required.     │
╰─────────────────────────────────────────────╯
```

---

## Installation

### From crates.io

```bash
cargo install jtt
```

### From source

```bash
git clone https://github.com/rodolfovillaruz/jtt
cd jtt
cargo install --path .
```

---

## Usage

```
jtt <input>
```

| Argument | Description                  |
|----------|------------------------------|
| `input`  | Path to a JSON chat file     |

### Examples

```bash
# View a chat file
jtt conversation.json

# Pipe to a file
jtt conversation.json > chat.txt

# Use a custom pager
PAGER=bat jtt conversation.json
```

---

## Input format

The JSON file must be either a **single object** or an **array of objects**.
Each object may carry the following fields:

| Field     | Type   | Required | Description                                      |
|-----------|--------|----------|--------------------------------------------------|
| `role`    | string | yes      | `"user"`, `"assistant"`, or `"system"`           |
| `content` | string | yes      | The message text (newlines and tabs are handled) |

### Array format (typical)

```json
[
  { "role": "system",    "content": "You are a helpful assistant." },
  { "role": "user",      "content": "Hello!" },
  { "role": "assistant", "content": "Hi there! How can I help you today?" }
]
```

### Single-object format

```json
{ "role": "user", "content": "Just one message." }
```

---

## Built-in pager keys

Invoked automatically when no external pager is found.

| Key                        | Action              |
|----------------------------|---------------------|
| `q` / `Q` / `Ctrl-C`       | Quit                |
| `↓` / `j` / `Enter`        | Scroll one line down |
| `↑` / `k`                  | Scroll one line up  |
| `PgDn` / `Space` / `f`     | Scroll one page down |
| `PgUp` / `b`               | Scroll one page up  |
| `g` / `Home`               | Jump to top         |
| `G` / `End`                | Jump to bottom      |

---

## License

MIT
