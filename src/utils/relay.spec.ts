import { expect, it } from "vitest";
import { normalizeRelayAddress } from "./relay";

it.each([
  [" http://10.129.81.21:8080/ ", "ws://10.129.81.21:8080"],
  ["https://relay.example:443", "ws://relay.example:443"],
  ["WSS://relay.example:443/", "ws://relay.example:443"],
  ["relay.example:8080", "ws://relay.example:8080"],
])("normalizes %s for the connection preview and save", (input, expected) => {
  expect(normalizeRelayAddress(input)).toBe(expected);
});

it.each(["http://relay.example/r/SHOW/presenter?token=x", "ftp://relay.example", "http://user:secret@relay.example", ""])('rejects an invalid server address: %s', input => {
  expect(() => normalizeRelayAddress(input)).toThrow();
});
