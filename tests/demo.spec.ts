import { test, expect } from '@playwright/test';
test('all pages render without runtime errors or horizontal overflow', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Ваши аккаунты. Всё вместе.' })).toBeVisible();
  for (const name of ['Аккаунты','Запуск','Серверы','Производительность','Мониторинг','Discord','Настройки','Диагностика']) {
    await page.locator('.sidebar').getByRole('button', { name: new RegExp(name), exact: false }).click();
    await expect(page.locator('h1')).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  expect(errors).toEqual([]);
});
test('demo account can be added, edited and removed', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Добавить аккаунт', exact: true }).click();
  await page.getByRole('button', { name: 'Добавить демо-аккаунт' }).click();
  await page.getByRole('button', { name: 'Добавить демо-аккаунт' }).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await page.locator('.sidebar').getByRole('button', { name: /Аккаунты/ }).click();
  await page.getByRole('button', { name: /Player 7.*player\.7/ }).click();
  await page.getByLabel('Псевдоним', { exact: true }).fill('Test player');
  await page.getByRole('button', { name: 'Сохранить изменения' }).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await page.getByRole('button', { name: /Test player.*player\.7/ }).click();
  await page.getByRole('button', { name: 'Удалить', exact: true }).click();
  await page.getByRole('button', { name: 'Подтвердить', exact: true }).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
});
test('mobile navigation and language switch work', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Menu' }).click();
  await page.locator('.sidebar').getByRole('button', { name: 'Настройки', exact: true }).click();
  await page.getByLabel('Язык', { exact: true }).selectOption('en');
  await page.getByRole('button', { name: 'Сохранить изменения', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});
