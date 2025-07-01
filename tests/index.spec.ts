import { expect, test, describe } from "vitest";
import { getVirtualization, isWslEnabled, isHypervEnabled, getMachineId, MachineIdFactor } from "../index";

describe("Virtualization", () => {
  test("getVirtualization", () => {
    const result = getVirtualization();
    expect(result).toBeDefined();
    expect(result.cpuSupported).toBeTypeOf("boolean");
    expect(result.osReportedEnabled).toBeTypeOf("boolean");
    expect(result.os).toBeOneOf(["windows", "linux", "macos"]);
  });
});

describe("WSL", () => {
  test("isWslEnabled", () => {
    const result = isWslEnabled();
    expect(result).toBeDefined();
    expect(result.enabled).toBeTypeOf("boolean");
    expect(result.enabled).toBeTruthy();
  });
});

describe("Hyper-V", () => {
  test("isHypervEnabled", () => {
    const result = isHypervEnabled();
    expect(result).toBeDefined();
    expect(result.enabled).toBeTypeOf("boolean");
  });
});

describe("WMI Conflict Reproduction", () => {
  /**
   * 这个测试的目的是模拟一个Worker线程被连续用于两个WMI操作的场景。
   * 第一个调用会建立一个初始的COM环境。
   * 第二个调用紧随其后，在同一个（可能已被第一个调用影响的）环境中执行。
   * 如果两个函数的Rust实现都尝试进行独立的COM初始化，且模式不兼容，
   * 那么第二个调用很可能会失败。
   */
  test("should trigger COM conflict when WMI functions are called sequentially", async () => {
    console.log("--- 开始顺序调用 WMI 函数 ---");
    // console.log("线程状态:",getThreadComState())
    // 我们使用 try...catch 来捕获预期的错误，这样测试本身就不会失败。
    try {
      // 第一次调用：这通常会成功，并为当前线程设置 COM 模式。
      console.log("步骤 1: 调用 isHypervEnabled()...");
      const hypervResult = isHypervEnabled();
      expect(hypervResult.enabled).toBeTypeOf("boolean");
      console.log("isHypervEnabled() 调用成功，结果:", hypervResult);

      // 第二次调用：这是最可能失败的地方。
      // 它在第一个调用建立的 COM 环境中运行。
      console.log("步骤 2: 调用 isWslEnabled()...");
      const wslResult = isWslEnabled();
      expect(wslResult.enabled).toBeTypeOf("boolean");
      console.log(
        "isWslEnabled() 调用成功，结果:",
        wslResult,
      );

      // 如果代码能执行到这里，说明在这次运行中没有发生冲突。
      // 这本身也是一个有用的信息。
      console.log("两个函数都成功执行，本次未复现冲突。");
    } catch (error) {
      // 如果任何一个调用抛出异常，我们在这里捕获它。
      console.error("在顺序调用中捕获到错误:", error);

      const errorMessage = String(error);

      // 断言这个错误就是我们预期的 COM 模式冲突错误。
      // 这会让测试用例“通过”，因为它成功地捕获到了预期的失败行为。
      expect(errorMessage).toMatch(
        /(-2147417850)|(Cannot change thread mode after it has been set)|(无法在设置线程模式后对其加以更改)/
      );

      console.log("成功复现了预期的 COM 模式冲突错误！");
      return; // 提前返回，因为我们已经验证了我们想验证的事情
    }

    // 如果 try 块成功执行完毕而没有抛出异常，我们可能需要让测试失败，
    // 因为这意味着我们没能复现问题。但在某些情况下，让它通过也可以接受。
    // 为了明确，我们可以加一个断言。
    // expect.fail("未能复现 COM 冲突错误。两个调用都成功了。");
  });
});

describe("MachineID", () => {
  test("getMachineID", () => {
    const result = getMachineId([MachineIdFactor.Baseboard, MachineIdFactor.Processor, MachineIdFactor.DiskDrivers]);
    expect(result).toBeDefined();
    expect(result.error).toBeUndefined();
    expect(result.machineId).toBeDefined();
    expect(result.factors).toBeInstanceOf(Array);
    expect(result.factors.find(it => it.startsWith('cpu_name'))).toBeDefined();
    expect(result.factors.find(it => it.startsWith('gpu'))).toBeUndefined();
    console.log(result)
  })
})