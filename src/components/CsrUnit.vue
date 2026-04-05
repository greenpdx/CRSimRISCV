<template>
  <div class="csr">
    <div class="csr__header">
      <span class="csr__title">CSR Registers</span>
    </div>
    <div class="csr__body">
      <div class="csr__row" v-for="reg in csrList" :key="reg.addr">
        <span class="csr__name">{{ reg.name }}</span>
        <span class="csr__addr">{{ reg.addrHex }}</span>
        <span class="csr__value">0x00000000</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const csrList = [
  { name: 'cycle',   addr: 0xC00, addrHex: 'C00' },
  { name: 'instret', addr: 0xC02, addrHex: 'C02' },
  { name: 'mstatus', addr: 0x300, addrHex: '300' },
  { name: 'mtvec',   addr: 0x305, addrHex: '305' },
  { name: 'mepc',    addr: 0x341, addrHex: '341' },
  { name: 'mcause',  addr: 0x342, addrHex: '342' },
]
</script>

<style scoped>
.csr {
  display: flex;
  flex-direction: column;
  background: #4ec3d5;
  border: 1.5px solid #2aacbe;
  border-radius: 3px;
  overflow: hidden;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
  width: max-content;
}

.csr__header {
  background: #2aacbe;
  padding: 2px 8px;
}
.csr__title {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 1px;
  color: #ffffff;
  text-transform: uppercase;
}

.csr__body {
  padding: 2px 0;
}
.csr__row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 6px;
}
.csr__row:nth-child(even) {
  background: #e8f8fb;
}
.csr__name {
  font-size: 9px;
  font-weight: 600;
  color: #0a2030;
  width: 52px;
}
.csr__addr {
  font-size: 8px;
  color: #6a9ab0;
  width: 24px;
}
.csr__value {
  font-size: 9px;
  color: #0a2030;
  font-weight: 500;
}
</style>
