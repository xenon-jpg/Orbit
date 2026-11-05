# Windows build / сборка Windows

Требуется Windows 10/11 **x64**, не ARM. WebView2 Runtime должен быть установлен. Сборка от обычного пользователя; права администратора для приложения не требуются.

1. Установите Node.js 22 LTS и Git.
2. Установите Visual Studio Build Tools 2022 → Desktop development with C++, актуальный MSVC и Windows 10/11 SDK.
3. Установите Rust через rustup, toolchain `stable-x86_64-pc-windows-msvc`.
4. Распакуйте исходники в путь без чужих файлов данных. В корне проекта:

```powershell
rustup target add x86_64-pc-windows-msvc
npm ci
npm run check
npx playwright install chromium
npm run test:e2e
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml --lib
npm run desktop:build
```

Для запуска в разработке: `npm run desktop:dev`. Для установщика: `npm run desktop:build`. Ожидаемый каталог результата — `src-tauri/target/release/bundle/nsis/`.

Если сборка не проходит, сохраните **первые** сообщения `error[...]`, а не только итог. Проверьте C++ Build Tools, SDK, версию Rust и свободную память. Рекомендуемый минимум для сборки — 8 GB RAM. Не отключайте проверку подписи исполняемого файла ради обхода ошибки.

Установщик не подписан сертификатом Authenticode. Windows может показать предупреждение. Проверяйте происхождение архива и SHA-256, не отключайте защиту системы.

Эта инструкция не является утверждением, что установщик уже собран или проверен. См. VERIFICATION.md и WINDOWS_ACCEPTANCE.md.
