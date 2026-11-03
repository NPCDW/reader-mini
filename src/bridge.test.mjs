import assert from "node:assert/strict";
import test from "node:test";

test("版本号通过 Tauri 接口读取，失败时可由页面兜底", async (t) => {
  const previousWindow = globalThis.window;
  t.after(() => {
    if (previousWindow === undefined) delete globalThis.window;
    else globalThis.window = previousWindow;
  });

  const calls = [];
  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd, args) => {
        calls.push({ cmd, args });
        return "9.8.7";
      },
    },
  };
  const { getAppVersion } = await import("./bridge.js?tauri");

  await t.test("读取运行中的应用版本，不使用固定版本号", async () => {
    assert.equal(await getAppVersion(), "9.8.7");
    assert.deepEqual(calls, [{ cmd: "plugin:app|version", args: {} }]);
  });

  await t.test("接口失败时传回错误", async () => {
    window.__TAURI_INTERNALS__.invoke = async () => {
      throw new Error("版本读取失败");
    };
    await assert.rejects(getAppVersion(), /版本读取失败/);
  });

  await t.test("浏览器预览时不调用 Tauri", async () => {
    globalThis.window = {};
    const browserBridge = await import("./bridge.js?browser");
    await assert.rejects(browserBridge.getAppVersion(), /不在 Tauri 环境里/);
  });
});
