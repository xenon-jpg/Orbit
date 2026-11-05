# Публикация в GitHub

В чате подключение GitHub недоступно, поэтому репозиторий не создан и push не выполнялся. Токены в исходниках и переписке не нужны.

## Вариант 1 — GitHub CLI
1. Создайте пустой репозиторий без README/license на GitHub, например `altdeck`.
2. Установите GitHub CLI и выполните `gh auth login` на своём компьютере.
3. Из распакованного проекта:

```powershell
./scripts/publish-github.ps1 -Repository "YOUR_LOGIN/altdeck" -Visibility private
```

Скрипт проверяет ввод, инициализирует Git при необходимости, делает commit и push. Он не публикует существующий dirty working tree молча и не меняет видимость репозитория после создания.

## Вариант 2 — обычный Git
Создайте пустой remote на GitHub, затем:

```powershell
git init -b main
git add .
git status --short
# Убедитесь, что нет vault.json, .altdeck, .env, cookies, target или node_modules.
git commit -m "Initial independent AltDeck experimental source"
git remote add origin https://github.com/YOUR_LOGIN/altdeck.git
git push -u origin main
```

Не используйте `git add -f` для пользовательских данных. После push откройте Actions и дождите результата Windows build. Это первый реальный Windows compile gate; не объявляйте релиз рабочим, пока он и ручной чек-лист не пройдены.
