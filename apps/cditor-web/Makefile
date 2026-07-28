.PHONY: help dev build-wasm build-web clean install

help: ## 显示帮助
	@echo "Cditor Web - 可用命令:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

install: ## 安装依赖
	@rustup target add wasm32-unknown-unknown || true
	@cargo install wasm-bindgen-cli || true
	@cd www && npm install

build-wasm: ## 构建 WASM（debug 模式）
	@cargo build --manifest-path Cargo.toml --target wasm32-unknown-unknown
	@mkdir -p www/src/wasm
	@wasm-bindgen \
		../../target/wasm32-unknown-unknown/debug/cditor_web.wasm \
		--out-dir www/src/wasm \
		--target web \
		--no-typescript

build-wasm-release: ## 构建 WASM（release 模式）
	@cargo build --manifest-path Cargo.toml --target wasm32-unknown-unknown --release
	@mkdir -p www/src/wasm
	@wasm-bindgen \
		../../target/wasm32-unknown-unknown/release/cditor_web.wasm \
		--out-dir www/src/wasm \
		--target web \
		--no-typescript

dev: build-wasm ## 启动开发服务器
	@cd www && npm install && npm run dev

clean: ## 清理构建产物
	@rm -rf www/src/wasm
	@cargo clean
