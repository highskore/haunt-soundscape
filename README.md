<div align="center">

<img src="docs/icon.png" width="96" alt="">

# Soundscape

**Hear your agents: a chime when one needs you, a bell when it's done, optional rain.**

[![Haunt plugin](https://img.shields.io/badge/Haunt-plugin-D97757)](https://github.com/highskore/haunt-hackathon)
![Plugin API 0.5](https://img.shields.io/badge/plugin%20API-0.5-111)
![WASM component](https://img.shields.io/badge/WASM-component-654ff0?logo=webassembly&logoColor=white)
![MIT licence](https://img.shields.io/badge/licence-MIT-6b7280)

</div>

Each agent gets its own soft chime, a note of the C major pentatonic picked from its name, so you can hear who needs you without looking: api is always the same note. A soft bell rings when an agent finishes, a fourth lower when its turn fails. Turn on `rain` for a quiet rain that thickens as more agents run and stops when they all do.

The sounds are Haunt's own (chime, bell and rain); Haunt's Sound setting can mute every plugin.

## Settings

```toml
[plugins."dev.haunt.soundscape"]
chimes = true       # each agent's chime when it needs you
bells = true        # a bell when an agent finishes
rain = false        # rain that thickens with running agents
volume = 50         # 0 to 100
rain_full_at = 6    # running agents for the heaviest rain
unverified = false  # also follow events without the tab's nonce
```

## Capabilities

| capability | why |
|---|---|
| `events` (session start, prompt, subagent, tool, permission request, question, stop) | each agent's state; no event text is kept |
| `sound` | plays Haunt's chime, bell and rain |
| `timer` | retries a refused rain change, so rain never outlasts the agents |

`[plugin_fx] sound = false` in Haunt's config mutes every plugin sound and speech.

## Build and test

```sh
cargo build --release --target wasm32-wasip2   # or ./build.sh
cargo test                                     # unit tests
haunt plugin install .
```

`e2e/soundscape.rs` runs the built plugin against a real Haunt core: copy it into a Haunt checkout's `cli/tests/` and run `HAUNT_PLUGINS_DIR=<this repo's parent> cargo test -p haunt-cli --features plugins --test soundscape`.

Needs Haunt with plugin API 0.5 (`api = "0.5"`) and its `sound` import.

## Licence

MIT, see `LICENSE`.
