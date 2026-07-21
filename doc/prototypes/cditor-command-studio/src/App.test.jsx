import React from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App.jsx";

afterEach(cleanup);

describe("Cditor Command Studio prototype", () => {
  it("collapses and expands the navigation sidebar", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "收起侧边栏" }));
    expect(screen.queryByPlaceholderText("搜索文档、页面和模板")).toBeNull();

    await user.click(screen.getByRole("button", { name: "展开侧边栏" }));
    expect(screen.getByPlaceholderText("搜索文档、页面和模板")).toBeTruthy();
  });

  it("toggles the command palette with the primary keyboard shortcut", () => {
    render(<App />);
    expect(screen.getByRole("dialog", { name: "命令面板" })).toBeTruthy();

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "命令面板" })).toBeNull();

    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(screen.getByRole("dialog", { name: "命令面板" })).toBeTruthy();
  });

  it("updates checklist state", async () => {
    const user = userEvent.setup();
    render(<App />);
    const task = screen.getByRole("button", { name: "实现增量渲染与虚拟化策略" });
    const indicator = task.querySelector("span");

    expect(indicator.classList.contains("checked")).toBe(false);
    await user.click(task);
    expect(indicator.classList.contains("checked")).toBe(true);
  });
});
