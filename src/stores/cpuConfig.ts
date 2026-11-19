// ─────────────────────────────────────────────────────────────────────────────
// CPU Configuration — feature flags and settings
// Persists to localStorage
// ───────────────���───────────────────────────────────��─────────────────────────

import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

const STORAGE_KEY = 'cpuvis_config'

export const useCpuConfig = defineStore('cpuConfig', () => {

  // ── Feature flags ──────────────────────────────────────────────────────────
  const enableCsr = ref(false)        // Zicsr: CSR instructions + registers
  const multicycle = ref(false)       // show the multicycle datapath panel
  const regMode = ref<8 | 32>(8)      // 8-register or 32-register view
  const ioBaseAddress = ref(0x40020000)
  const romSize = ref(32)
  const ramSize = ref(32)

  // ── Persistence ───────────────���────────────────────────────────────────────

  function load() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (!raw) return
      const cfg = JSON.parse(raw)
      if (cfg.enableCsr !== undefined) enableCsr.value = cfg.enableCsr
      if (cfg.multicycle !== undefined) multicycle.value = cfg.multicycle
      if (cfg.regMode !== undefined) regMode.value = cfg.regMode
      if (cfg.ioBaseAddress !== undefined) ioBaseAddress.value = cfg.ioBaseAddress
      if (cfg.romSize !== undefined) romSize.value = cfg.romSize
      if (cfg.ramSize !== undefined) ramSize.value = cfg.ramSize
    } catch { /* ignore */ }
  }

  function save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({
        enableCsr: enableCsr.value,
        multicycle: multicycle.value,
        regMode: regMode.value,
        ioBaseAddress: ioBaseAddress.value,
        romSize: romSize.value,
        ramSize: ramSize.value,
      }))
    } catch { /* ignore */ }
  }

  // Auto-save on change
  watch([enableCsr, multicycle, regMode, ioBaseAddress, romSize, ramSize], save, { deep: true })

  // Load on init
  load()

  return {
    enableCsr,
    multicycle,
    regMode,
    ioBaseAddress,
    romSize,
    ramSize,
    load,
    save,
  }
})
