ADDR ?= 0.0.0.0:3000
BIN := $(or $(CARGO_INSTALL_ROOT),$(CARGO_HOME),$(HOME)/.cargo)/bin/audio-drama-editor
OS := $(shell uname -s)

# GPU: macOS は Metal、NVIDIA GPU のある Linux は CUDA、それ以外は CPU
ifeq ($(OS),Darwin)
FEATURES ?= metal
else ifneq ($(shell command -v nvidia-smi 2>/dev/null),)
FEATURES ?= cuda
else
FEATURES ?=
endif

UNIT := $(HOME)/.config/systemd/user/audio-drama-editor.service
LABEL := io.github.kobago.audio-drama-editor
PLIST := $(HOME)/Library/LaunchAgents/$(LABEL).plist
LOG := $(HOME)/Library/Logs/audio-drama-editor.log

.PHONY: help frontend install service restart stop start status logs uninstall test

help:
	@echo 'make install           build the frontend and install audio-drama-editor to $(BIN) (FEATURES=$(FEATURES))'
	@echo 'make service [ADDR=]   register and start the user service (default ADDR=$(ADDR))'
	@echo 'make restart           restart the service after make install'
	@echo 'make stop | start      stop the service until make start (it also stays off at login)'
	@echo 'make status | logs     show the service state or follow its log'
	@echo 'make uninstall         stop and remove the service (data in ~/.audio-drama-editor is kept)'
	@echo 'make test              run backend, frontend and end-to-end tests'

frontend:
	cd frontend && bun install --frozen-lockfile && bun run build

install: frontend
	cargo install --locked --path crates/server $(if $(FEATURES),--features $(FEATURES))

ifeq ($(OS),Darwin)
service:
	@test -x $(BIN) || { echo '$(BIN) not found; run make install first' >&2; exit 1; }
	mkdir -p $(dir $(PLIST)) $(dir $(LOG))
	sed -e 's|@BIN@|$(BIN)|' -e 's|@ADDR@|$(ADDR)|' -e 's|@LOG@|$(LOG)|' deploy/$(LABEL).plist > $(PLIST)
	-launchctl bootout gui/$$(id -u)/$(LABEL) 2>/dev/null
	launchctl enable gui/$$(id -u)/$(LABEL)
	launchctl bootstrap gui/$$(id -u) $(PLIST)

restart:
	launchctl kickstart -k gui/$$(id -u)/$(LABEL)

# disable しておかないと、次のログインで LaunchAgents の plist からまた起動する
stop:
	launchctl disable gui/$$(id -u)/$(LABEL)
	launchctl bootout gui/$$(id -u)/$(LABEL)

start:
	@test -f $(PLIST) || { echo '$(PLIST) not found; run make service first' >&2; exit 1; }
	launchctl enable gui/$$(id -u)/$(LABEL)
	launchctl bootstrap gui/$$(id -u) $(PLIST)

status:
	launchctl print gui/$$(id -u)/$(LABEL) | head -20

logs:
	tail -f $(LOG)

uninstall:
	-launchctl bootout gui/$$(id -u)/$(LABEL)
	rm -f $(PLIST)
else
service:
	@test -x $(BIN) || { echo '$(BIN) not found; run make install first' >&2; exit 1; }
	mkdir -p $(dir $(UNIT))
	sed -e 's|@BIN@|$(BIN)|' -e 's|@ADDR@|$(ADDR)|' deploy/audio-drama-editor.service > $(UNIT)
	systemctl --user daemon-reload
	systemctl --user enable audio-drama-editor
	systemctl --user restart audio-drama-editor
	@loginctl show-user $$USER --property=Linger 2>/dev/null | grep -q yes || \
		echo 'note: run "sudo loginctl enable-linger $$USER" so audio-drama-editor starts at boot without a login'

restart:
	systemctl --user restart audio-drama-editor

stop:
	systemctl --user disable --now audio-drama-editor

start:
	systemctl --user enable --now audio-drama-editor

status:
	systemctl --user status audio-drama-editor

logs:
	journalctl --user -u audio-drama-editor -f

uninstall:
	-systemctl --user disable --now audio-drama-editor
	rm -f $(UNIT)
	systemctl --user daemon-reload
endif

test:
	cargo test --workspace
	cd frontend && bun run test && bun run test:e2e
