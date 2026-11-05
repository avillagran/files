# Later assessment of the result

The second FAIL is the harness's: it looked for fixture_note="preserve" without spaces, although config.toml keeps the value through TOML serialization with spaces. The three processes were not yet part of this scope: this run had two normal exits and the earlier captures. The original FAIL is kept.
