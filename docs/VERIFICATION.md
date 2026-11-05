# Проверки в среде разработки

Дата: 2026-11-03. Среда: Linux x64, около 2 GB RAM, Node 20, Rust stable 1.99.1. Это **не** Windows runtime validation.

- `npm run build`: успешно, TypeScript + Vite production bundle.
- `npm test`: 8/8 unit tests.
- `npm run test:e2e`: 3/3 Chromium Playwright tests; девять экранов, отсутствие горизонтального переполнения, добавление/редактирование/удаление демо-аккаунта, мобильная навигация и смена языка.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 15/15 тестов на Linux host. Windows-only модули при этом не компилируются.
- Проверка Windows target `x86_64-pc-windows-msvc` дважды прервана SIGKILL при компиляции зависимости `windows 0.62.2` из-за нехватки памяти. Повтор с одним заданием и отключёнными debug symbols также не завершился. **Ошибки приложения не были доказаны или исключены этой проверкой.**
- Реальный Roblox, DPAPI, Windows Hello, WebView2 login, Job Objects, PrintWindow, multi-instance и NSIS installer здесь не запускались.
- Реальные аккаунты, cookies и webhook не использовались.

Скриншоты интерфейса находятся в `docs/screenshots/`. На них именно помеченное браузерное демо, не скриншот работающего Roblox.

Перед публикацией рабочего релиза требуется зелёный Windows CI и ручное прохождение WINDOWS_ACCEPTANCE.md. Наличие тестов не является аудитом безопасности или гарантией совместимости.
