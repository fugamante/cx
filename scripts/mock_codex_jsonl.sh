#!/usr/bin/env bash
set -euo pipefail

prompt="$(cat)"

if [[ "$prompt" == *'"commands"'* ]] || [[ "$prompt" == *"cxnext"* ]]; then
  text='{"commands":["echo ok"]}'
else
  text='ok'
fi

json_text="$(python3 -c 'import json, sys; print(json.dumps(sys.argv[1]))' "$text")"

printf '%s\n' "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":${json_text}}}"
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
