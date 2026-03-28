APP_NAME = AitopMenuBar
APP_DIR = /Applications/$(APP_NAME).app
CONTENTS_DIR = $(APP_DIR)/Contents
MACOS_DIR = $(CONTENTS_DIR)/MacOS

.PHONY: build install uninstall clean

build:
	swift build -c release

install: build
	@mkdir -p $(MACOS_DIR)
	@cp .build/release/$(APP_NAME) $(MACOS_DIR)/$(APP_NAME)
	@cp Info.plist $(CONTENTS_DIR)/Info.plist
	@echo "Installed to $(APP_DIR)"
	@echo "Run: open $(APP_DIR)"

uninstall:
	@rm -rf $(APP_DIR)
	@echo "Uninstalled $(APP_NAME)"

clean:
	swift package clean
