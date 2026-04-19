# jtt

**Pretty-print JSON chat logs as left/right terminal bubbles with a built-in pager.**

[![Crates.io](https://img.shields.io/crates/v/jtt)](https://crates.io/crates/jtt)
[![License: MIT](https://img.shields.io/crates/l/jtt)](https://crates.io/crates/jtt)

## What is this?

`jtt` is a terminal viewer for JSON conversation files. It renders each message
as a Unicode-bordered bubble — user messages float to the right, assistant
messages to the left, system messages span the full width — and opens the result
in a keyboard-driven, full-screen pager so you can scroll through long
conversations without losing context.

It pairs naturally with [raw-llm](https://github.com/rodolfovillaruz/raw-llm),
which stores every conversation as a plain JSON file in exactly the format `jtt`
expects.

## Installation

### From [crates.io](https://crates.io/crates/jtt)

```bash
cargo install jtt
```

### From source

```bash
git clone https://github.com/rodolfovillaruz/jtt.git
cd jtt
cargo install --path .
```

## Usage

```bash
jtt path/to/conversation.json
```

`jtt` reads the file, wraps every message to 75 % of your terminal width, and
opens the built-in pager. When stdout is redirected to a pipe or file the pager
is skipped and the rendered text is written directly.

```bash
# Pipe the rendered output to a file
jtt conversation.json > conversation.txt
```

### Pager key bindings

| Key                        | Action            |
| -------------------------- | ----------------- |
| `q` / `Q` / `Ctrl-C`       | Quit              |
| `↓` / `j` / `Enter`        | One line down     |
| `↑` / `k`                  | One line up       |
| `PgDn` / `Space` / `f`     | One page down     |
| `PgUp` / `b`               | One page up       |
| `g` / `Home`               | Jump to top       |
| `G` / `End`                | Jump to bottom    |

## Conversation format

`jtt` reads JSON files that are either a single message object or an array of
message objects. Each object needs a `role` and a `content` field:

```json
[
  { "role": "system",    "content": "You are a helpful assistant." },
  { "role": "user",      "content": "What is context engineering?" },
  { "role": "assistant", "content": "Context engineering is the practice of …" }
]
```

| `role`      | Bubble position        |
| ----------- | ---------------------- |
| `user`      | Right (75 % width)     |
| `assistant` | Left  (75 % width)     |
| `system`    | Full terminal width    |
| anything else | Left (75 % width)   |

## Using with raw-llm

[raw-llm](https://github.com/rodolfovillaruz/raw-llm) saves every conversation
as a JSON array in the same format `jtt` expects. The two tools compose
naturally over a shared file.

### Start a conversation, then view it

```bash
# Start a new conversation with Claude
echo "Explain monads in one paragraph" | claude .prompt/monads.json

# View the conversation as chat bubbles
jtt .prompt/monads.json
```

### Continue a conversation, then view it

```bash
# Add another turn to an existing conversation
echo "Give me a concrete Haskell example" | claude .prompt/monads.json

# Review the full exchange
jtt .prompt/monads.json
```

### Pipe the rendered output

```bash
# Render to plain text for sharing or archiving
jtt .prompt/monads.json > monads.txt
```

### Typical workflow

```bash
# 1 · Ask a question
echo "Refactor this function" | cat main.rs - | claude .prompt/refactor.json

# 2 · Review the answer in the pager
jtt .prompt/refactor.json

# 3 · Edit the conversation file to steer context, then continue
$EDITOR .prompt/refactor.json
echo "Now add error handling" | claude .prompt/refactor.json

# 4 · Review the updated exchange
jtt .prompt/refactor.json
```

## License

MIT
