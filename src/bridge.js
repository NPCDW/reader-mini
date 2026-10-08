// Tauri 前端桥：把命令调用与事件订阅收在一处，UI 只关心数据。
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function call(cmd, args = {}) {
  if (!isTauri) return Promise.reject(new Error("不在 Tauri 环境里"));
  return invoke(cmd, args);
}

export function send(event, payload) {
  if (!isTauri) return Promise.resolve();
  return emit(event, payload);
}

export function on(event, handler) {
  if (!isTauri) return Promise.resolve(() => {});
  return listen(event, handler);
}

export const getConfig = () => call("get_config");
export const saveConfig = (config) => call("save_config", { config });
export const getBookshelf = () => call("get_bookshelf");
export const getChapterList = (bookUrl) =>
  call("get_chapter_list", { bookUrl });
export const cacheBooks = (books) => call("cache_books", { books });
export const setCurrentBook = (index) => call("set_current_book", { index });
export const resumePoint = (bookIndex) => call("resume_point", { bookIndex });
export const pollToggle = () => call("poll_toggle");
export const takePending = () => call("take_pending");
export const openReader = (args) => call("open_reader", args);
export const switchChapter = (args) => call("switch_chapter", args);
export const closeReader = (args) => call("close_reader", args);
export const saveReaderSize = (width, height) =>
  call("save_reader_size", { width, height });
export const refreshReaderStyle = () => call("refresh_reader_style");
