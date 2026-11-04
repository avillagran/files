.PHONY: build test memcheck
build:
	cargo build --release
test:
	cargo test --workspace
memcheck: build
	python3 scripts/memcheck.py

.PHONY: html-helper
# Optional development adapter; builds verified native source, never installs globally.
html-helper:
	python3 scripts/html-engine-probe.py --build
	python3 scripts/stage-html-helper.py
