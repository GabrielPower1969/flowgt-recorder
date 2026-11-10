import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { bindRouter, getRoute, navigate, parseHash } from "./router.svelte";

function fireHashChange() {
  window.dispatchEvent(new HashChangeEvent("hashchange"));
}

describe("parseHash", () => {
  it("treats empty and root hashes as /", () => {
    expect(parseHash("")).toBe("/");
    expect(parseHash("#")).toBe("/");
    expect(parseHash("#/")).toBe("/");
  });

  it("recognises known routes", () => {
    expect(parseHash("#/settings")).toBe("/settings");
    expect(parseHash("#/onboarding")).toBe("/");
    expect(parseHash("#/summary")).toBe("/summary");
    expect(parseHash("#/floating")).toBe("/floating");
  });

  it("falls back to / for unknown routes", () => {
    expect(parseHash("#/nope")).toBe("/");
    expect(parseHash("#/settings/deeper")).toBe("/");
  });

  it("accepts a hash without the leading slash", () => {
    expect(parseHash("#settings")).toBe("/settings");
  });

  it("ignores a query string", () => {
    expect(parseHash("#/summary?id=3")).toBe("/summary");
  });
});

describe("navigate + bindRouter", () => {
  let unbind: () => void = () => {};

  beforeEach(() => {
    location.hash = "";
    unbind = bindRouter();
  });
  afterEach(() => {
    unbind();
    location.hash = "";
  });

  it("bindRouter reads the current hash", () => {
    expect(getRoute()).toBe("/");
    location.hash = "#/settings";
    fireHashChange();
    expect(getRoute()).toBe("/settings");
  });

  it("navigate updates location.hash and the route", () => {
    navigate("/settings");
    expect(location.hash).toBe("#/settings");
    fireHashChange();
    expect(getRoute()).toBe("/settings");

    navigate("/");
    expect(location.hash).toBe("#/");
    fireHashChange();
    expect(getRoute()).toBe("/");
  });

  it("navigate to the current hash still sets the route", () => {
    location.hash = "#/summary";
    fireHashChange();
    expect(getRoute()).toBe("/summary");
    navigate("/summary");
    expect(location.hash).toBe("#/summary");
    expect(getRoute()).toBe("/summary");
  });

  it("an unknown hash typed by hand resolves to /", () => {
    location.hash = "#/does-not-exist";
    fireHashChange();
    expect(getRoute()).toBe("/");
  });

  it("binding twice is a no-op and unbinding stops updates", () => {
    const second = bindRouter();
    second(); // no-op when already bound by the outer binding
    navigate("/settings");
    fireHashChange();
    expect(getRoute()).toBe("/settings");

    unbind();
    location.hash = "#/summary";
    fireHashChange();
    expect(getRoute()).toBe("/settings"); // listener removed
    unbind = bindRouter();
    expect(getRoute()).toBe("/summary"); // re-bind re-reads the hash
  });
});
