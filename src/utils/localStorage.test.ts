// `utils/localStorage.ts` 单测（node 环境，localStorage 用 stub）。
//
// 测什么：原始读写、键枚举顺序、字符串/布尔/数字/整数解析、范围钳制、脏值回退，
// 以及存储不可用或读写抛异常时的统一降级（隐私模式 / node 环境 / 配额满）。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

let store = new Map<string, string>();

function stubLocalStorage(): void {
  store = new Map();
  vi.stubGlobal("localStorage", {
    get length() {
      return store.size;
    },
    key: (i: number) => [...store.keys()][i] ?? null,
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
  });
}

beforeEach(() => {
  stubLocalStorage();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

import {
  listLocalStorageKeys,
  readLocalStorageBoolean,
  readLocalStorageInteger,
  readLocalStorageNumber,
  readLocalStorageString,
  readLocalStorageValue,
  writeLocalStorageBoolean,
  writeLocalStorageNumber,
  writeLocalStorageString,
  writeLocalStorageValue,
} from "./localStorage";

describe("原始读写", () => {
  it("缺失返回 null，写入后可读", () => {
    expect(readLocalStorageValue("missing")).toBeNull();
    writeLocalStorageValue("a", "1");
    expect(readLocalStorageValue("a")).toBe("1");
  });

  it("键枚举保持插入顺序", () => {
    writeLocalStorageValue("b", "2");
    writeLocalStorageValue("a", "1");
    expect(listLocalStorageKeys()).toEqual(["b", "a"]);
  });
});

describe("字符串解析", () => {
  it("缺失回退 fallback；空串默认保留，emptyAsFallback 时回退", () => {
    expect(readLocalStorageString("missing", "fallback")).toBe("fallback");
    writeLocalStorageString("empty", "");
    expect(readLocalStorageString("empty", "fallback")).toBe("");
    expect(readLocalStorageString("empty", "fallback", { emptyAsFallback: true })).toBe("fallback");
  });
});

describe("布尔解析", () => {
  it("两种默认语义都靠 fallback 表达", () => {
    expect(readLocalStorageBoolean("missing", true)).toBe(true);
    expect(readLocalStorageBoolean("missing", false)).toBe(false);
  });

  it("0/false 为假，1/true 为真，脏值回退", () => {
    writeLocalStorageValue("k", "0");
    expect(readLocalStorageBoolean("k", true)).toBe(false);
    writeLocalStorageValue("k", "false");
    expect(readLocalStorageBoolean("k", true)).toBe(false);
    writeLocalStorageValue("k", "1");
    expect(readLocalStorageBoolean("k", false)).toBe(true);
    writeLocalStorageValue("k", "true");
    expect(readLocalStorageBoolean("k", false)).toBe(true);
    writeLocalStorageValue("k", "yes");
    expect(readLocalStorageBoolean("k", true)).toBe(true);
  });

  it("写入统一为 1/0", () => {
    writeLocalStorageBoolean("k", true);
    expect(store.get("k")).toBe("1");
    writeLocalStorageBoolean("k", false);
    expect(store.get("k")).toBe("0");
  });
});

describe("数字解析", () => {
  it("缺失 / 空串 / 非数字回退 fallback", () => {
    expect(readLocalStorageNumber("missing", 7)).toBe(7);
    writeLocalStorageValue("blank", " ");
    expect(readLocalStorageNumber("blank", 7)).toBe(7);
    writeLocalStorageValue("bad", "abc");
    expect(readLocalStorageNumber("bad", 7)).toBe(7);
  });

  it("给了 range 则钳制到端点", () => {
    writeLocalStorageValue("n", "99");
    expect(readLocalStorageNumber("n", 5, { min: 2, max: 12 })).toBe(12);
    writeLocalStorageValue("n", "-3");
    expect(readLocalStorageNumber("n", 5, { min: 2, max: 12 })).toBe(2);
  });

  it("整数解析四舍五入", () => {
    writeLocalStorageValue("n", "3.6");
    expect(readLocalStorageInteger("n", 1)).toBe(4);
  });

  it("写入数字为字符串", () => {
    writeLocalStorageNumber("n", 2.5);
    expect(store.get("n")).toBe("2.5");
  });
});

describe("存储不可用时的降级", () => {
  it("没有 localStorage：读回 fallback，枚举为空，写入不抛", () => {
    vi.stubGlobal("localStorage", undefined);
    expect(readLocalStorageValue("k")).toBeNull();
    expect(readLocalStorageString("k", "fb")).toBe("fb");
    expect(readLocalStorageBoolean("k", true)).toBe(true);
    expect(readLocalStorageNumber("k", 5, { min: 0, max: 10 })).toBe(5);
    expect(listLocalStorageKeys()).toEqual([]);
    expect(() => writeLocalStorageValue("k", "v")).not.toThrow();
  });

  it("读写抛异常：按缺失处理，不向上抛", () => {
    vi.stubGlobal("localStorage", {
      get length(): number {
        throw new Error("blocked");
      },
      key: () => {
        throw new Error("blocked");
      },
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
      removeItem: () => {
        throw new Error("blocked");
      },
    });
    expect(readLocalStorageValue("k")).toBeNull();
    expect(readLocalStorageBoolean("k", false)).toBe(false);
    expect(listLocalStorageKeys()).toEqual([]);
    expect(() => writeLocalStorageValue("k", "v")).not.toThrow();
  });
});
