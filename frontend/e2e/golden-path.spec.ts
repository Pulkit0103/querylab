import { expect, test, type Page } from "@playwright/test";

/**
 * The path every user takes: create an account, add a database, run a
 * query, save it, find it again, sign out and back in. Runs against the real
 * API and a real PostgreSQL target (see playwright.config.ts).
 */

const target = new URL(process.env.E2E_DATABASE_URL ?? "postgres://querylab:querylab@127.0.0.1:5432/querylab");
target.searchParams.set("sslmode", "disable");

const password = "correct horse battery staple";
const email = `e2e-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.com`;

async function typeQuery(page: Page, sql: string) {
  const editor = page.locator(".monaco-editor").first();
  await expect(editor).toBeVisible();
  await editor.click();
  await page.keyboard.press("ControlOrMeta+A");
  await page.keyboard.press("Delete");
  await page.keyboard.type(sql);
  await page.keyboard.press("Escape"); // close any suggestion widget
}

test("protected pages redirect to sign-in", async ({ page }) => {
  await page.goto("/editor");
  await expect(page).toHaveURL(/\/login\?next=%2Feditor/);
  await expect(page.getByRole("heading", { name: "Sign in to QueryLab" })).toBeVisible();
  const me = await page.request.get("/api/auth/me");
  expect(me.status()).toBe(401);
});

test("register, connect, query, save, sign out and back in", async ({ page }) => {
  await test.step("create an account", async () => {
    await page.goto("/register");
    await page.getByLabel("Name").fill("End To End");
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Create account" }).click();
    await expect(page).toHaveURL(/\/editor$/);
  });

  await test.step("add a database", async () => {
    await page.getByRole("button", { name: "Add database" }).click();
    const dialog = page.getByRole("dialog", { name: "Add PostgreSQL connection" });
    await expect(dialog).toBeVisible();
    await dialog.getByLabel("Connection string").fill(target.toString());
    await dialog.getByLabel("Display name").fill("E2E database");
    await dialog.getByRole("button", { name: "Test connection" }).click();
    await expect(dialog.getByText(/Connected in/)).toBeVisible();
    await dialog.getByRole("button", { name: "Save connection" }).click();
    await expect(dialog).toBeHidden();
    await expect(page.getByRole("button", { name: "Active database" })).toContainText("E2E database");
  });

  await test.step("run a query", async () => {
    await typeQuery(page, "select 6 * 7 as answer, 'golden path' as label");
    await page.getByRole("button", { name: "Run", exact: true }).click();
    const results = page.getByLabel("Query results");
    await expect(results).toContainText("answer");
    await expect(results).toContainText("42");
    await expect(results).toContainText("golden path");
    await expect(page.getByRole("tabpanel", { name: "Results" }).getByText("1 rows")).toBeVisible();
  });

  await test.step("save it and find it in the library and history", async () => {
    await page.getByRole("button", { name: "Save query" }).click();
    const dialog = page.getByRole("dialog", { name: "Save query" });
    await dialog.getByLabel("Name").fill("The answer");
    await dialog.getByRole("button", { name: "Save query" }).click();
    await expect(dialog).toBeHidden();

    await page.goto("/saved");
    await expect(page.getByText("The answer").first()).toBeVisible();
    await page.goto("/history");
    await expect(page.getByText("select 6 * 7 as answer").first()).toBeVisible();
  });

  await test.step("sign out and back in", async () => {
    await page.getByRole("button", { name: "Account menu" }).click();
    await page.getByRole("menuitem", { name: "Sign out" }).click();
    await expect(page).toHaveURL(/\/login/);
    expect((await page.request.get("/api/auth/me")).status()).toBe(401);

    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Sign in" }).click();
    // Back on the page the user signed out from, with their data.
    await expect(page).toHaveURL(/\/history$/);
    await expect(page.getByText("select 6 * 7 as answer").first()).toBeVisible();
    await expect(page.getByRole("button", { name: "Active database" })).toContainText("E2E database");
  });
});
