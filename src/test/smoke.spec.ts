import { describe, it, expect } from "vitest";

describe("vitest infrastructure", () => {
  it("can run a basic test", () => {
    expect(1 + 1).toBe(2);
  });

  it("has jsdom environment", () => {
    const div = document.createElement("div");
    div.textContent = "ok";
    expect(div.textContent).toBe("ok");
  });
});
