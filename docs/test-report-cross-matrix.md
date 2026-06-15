# OpenLink 交叉测试矩阵报告

> 版本：v1.0.0
> 日期：2026-06-16
> 来源：WO-069 执行交叉测试矩阵（LLM×Extension×场景）
> 状态：🔧进行中

---

## 一、测试矩阵设计

### 1.1 矩阵维度

| 维度 | 选项 | 说明 |
|------|------|------|
| **LLM** | 3个 | 至少1个国产+1个国际+1个小模型 |
| **Extension** | 3个 | 核心Extension |
| **场景** | 3个 | 单Agent访问/多Agent协作/故障恢复 |

**最小组合数**：3 × 3 × 3 = 27个测试用例

### 1.2 LLM选择

| ID | 模型 | 类型 | 说明 |
|----|------|------|------|
| LLM-01 | GPT-4o | 国际大模型 | OpenAI最新旗舰 |
| LLM-02 | Claude-3.5 | 国际大模型 | Anthropic主力 |
| LLM-03 | Qwen2-7B | 国产小模型 | 阿里通义千问 |

### 1.3 Extension选择

| ID | Extension | 说明 |
|----|-----------|------|
| EXT-01 | Redirect | 重定向扩展（核心） |
| EXT-02 | Webhook | Webhook扩展（外部调用） |
| EXT-03 | FileTransfer | 文件传输扩展（状态修改） |

### 1.4 场景选择

| ID | 场景 | 说明 |
|----|------|------|
| SCN-01 | 单Agent访问 | 单一Agent调用Extension |
| SCN-02 | 多Agent协作 | 多个Agent协同调用Extension |
| SCN-03 | 故障恢复 | Extension失败后的恢复测试 |

---

## 二、测试用例清单

### 2.1 完整矩阵

| # | LLM | Extension | 场景 | 预期结果 | 优先级 |
|---|-----|-----------|------|----------|--------|
| TC-001 | GPT-4o | Redirect | 单Agent | ✅通过 | P0 |
| TC-002 | GPT-4o | Redirect | 多Agent | ✅通过 | P0 |
| TC-003 | GPT-4o | Redirect | 故障恢复 | ⚠️需验证 | P1 |
| TC-004 | GPT-4o | Webhook | 单Agent | ✅通过 | P0 |
| TC-005 | GPT-4o | Webhook | 多Agent | ⚠️需验证 | P1 |
| TC-006 | GPT-4o | Webhook | 故障恢复 | ⚠️需验证 | P2 |
| TC-007 | GPT-4o | FileTransfer | 单Agent | ✅通过 | P0 |
| TC-008 | GPT-4o | FileTransfer | 多Agent | ⚠️需验证 | P1 |
| TC-009 | GPT-4o | FileTransfer | 故障恢复 | ⚠️需验证 | P2 |
| TC-010 | Claude-3.5 | Redirect | 单Agent | ✅通过 | P0 |
| TC-011 | Claude-3.5 | Redirect | 多Agent | ✅通过 | P0 |
| TC-012 | Claude-3.5 | Redirect | 故障恢复 | ⚠️需验证 | P1 |
| TC-013 | Claude-3.5 | Webhook | 单Agent | ✅通过 | P0 |
| TC-014 | Claude-3.5 | Webhook | 多Agent | ⚠️需验证 | P1 |
| TC-015 | Claude-3.5 | Webhook | 故障恢复 | ⚠️需验证 | P2 |
| TC-016 | Claude-3.5 | FileTransfer | 单Agent | ✅通过 | P0 |
| TC-017 | Claude-3.5 | FileTransfer | 多Agent | ⚠️需验证 | P1 |
| TC-018 | Claude-3.5 | FileTransfer | 故障恢复 | ⚠️需验证 | P2 |
| TC-019 | Qwen2-7B | Redirect | 单Agent | ⚠️需验证 | P1 |
| TC-020 | Qwen2-7B | Redirect | 多Agent | ⚠️需验证 | P1 |
| TC-021 | Qwen2-7B | Redirect | 故障恢复 | ⚠️需验证 | P2 |
| TC-022 | Qwen2-7B | Webhook | 单Agent | ⚠️需验证 | P1 |
| TC-023 | Qwen2-7B | Webhook | 多Agent | ⚠️需验证 | P2 |
| TC-024 | Qwen2-7B | Webhook | 故障恢复 | ⚠️需验证 | P2 |
| TC-025 | Qwen2-7B | FileTransfer | 单Agent | ⚠️需验证 | P1 |
| TC-026 | Qwen2-7B | FileTransfer | 多Agent | ⚠️需验证 | P2 |
| TC-027 | Qwen2-7B | FileTransfer | 故障恢复 | ⚠️需验证 | P2 |

---

## 三、CI集成

### 3.1 GitHub Actions配置

```yaml
name: Cross Matrix Test

on:
  push:
    branches: [master]
  pull_request:
    branches: [master]
  schedule:
    - cron: '0 2 * * *'  # 每天2点

jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        llm: [gpt-4o, claude-3.5, qwen2-7b]
        extension: [redirect, webhook, file-transfer]
        scenario: [single-agent, multi-agent, fault-recovery]
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Rust
        uses: actions-rust-lang/setup-rust-toolchain@v1
      
      - name: Run test
        run: |
          cargo test --package openlink-core \
            --test cross_matrix \
            -- llm=${{ matrix.llm }} \
               extension=${{ matrix.extension }} \
               scenario=${{ matrix.scenario }}
      
      - name: Upload results
        uses: actions/upload-artifact@v4
        with:
          name: test-results-${{ matrix.llm }}-${{ matrix.extension }}-${{ matrix.scenario }}
          path: test-results/
```

---

## 四、合格标准

| 指标 | 目标 | 说明 |
|------|------|------|
| 核心路径通过率 | 100% | TC-001 ~ TC-018 |
| 扩展路径通过率 | ≥80% | TC-019 ~ TC-027 |
| 测试执行时间 | <10min | 完整矩阵 |
| 覆盖率 | 100% | 所有声明的组合 |

---

## 五、待办事项

- [ ] 实现自动化测试脚本
- [ ] 配置CI环境
- [ ] 执行完整测试矩阵
- [ ] 收集失败案例并分析
- [ ] 产出测试报告

---

*文档版本：v1.0.0-draft*
