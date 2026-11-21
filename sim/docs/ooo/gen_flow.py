"""Generate ooo-flow.svg — the OoO core's instruction flow + a worked timeline.
Usage: python3 gen_flow.py ooo-flow.svg   (run from sim/docs/ooo)
The timeline is computed from the core's rules (single issue, N_CDB result buses, oldest first,
wake-up the cycle after a broadcast, in-order commit)."""
import html, sys
E = html.escape
N_CDB = 2
prog = [  # text, unit, dest, srcs, latency
    ("lw   x4, 0(x10)", "LSU", "x4", ["x10"], 3),
    ("mul  x6, x2, x3", "MUL", "x6", ["x2", "x3"], 3),
    ("add  x5, x4, x1", "ALU", "x5", ["x4", "x1"], 1),
    ("addi x7, x0, 5",  "ALU", "x7", [], 1),
    ("sub  x8, x6, x7", "ALU", "x8", ["x6", "x7"], 1),
    ("sw   x8, 4(x10)", "LSU", None, ["x10", "x8"], 1),
]
# ── timeline, in program order: older instructions claim CDB slots first (= oldest first) ──
rows, prod, alu, lsu, cdb = [], {}, {}, set(), {}
for i, (n, u, d, src, lat) in enumerate(prog, 1):
    IF, DI = i, i + 1
    deps = [prod[s] for s in src if s in prod]
    IS = max([DI + 1] + [rows[p - 1]['WB'] + 1 for p in deps])    # wake-up: the cycle after the broadcast
    while (u == "LSU" and IS in lsu) or (u == "ALU" and alu.get(IS, 0) >= 2): IS += 1
    if u == "LSU": lsu.add(IS)
    if u == "ALU": alu[IS] = alu.get(IS, 0) + 1
    EX = IS + lat - 1
    WB = None
    if d:
        WB = EX + 1
        while cdb.get(WB, 0) >= N_CDB: WB += 1
        cdb[WB] = cdb.get(WB, 0) + 1
    rows.append(dict(n=n, u=u, d=d, IF=IF, DI=DI, IS=IS, EX=EX, WB=WB, deps=deps, lat=lat))
    if d: prod[d] = i
prev = 0
for r in rows:
    done = r['WB'] if r['WB'] else r['EX']
    r['CM'] = max(done + 1, prev + 1); prev = r['CM']
for r in rows: print(r, file=sys.stderr)

W, H = 1500, 960
out = []; a = out.append
a(f'''<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" font-family="Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif">
  <title>Out-of-order core (Tomasulo + ROB): instruction flow</title>
  <defs>
    <marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="#334155"/></marker>
    <marker id="aO" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="#d97706"/></marker>
    <marker id="aG" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="#15803d"/></marker>
    <marker id="aR" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="#dc2626"/></marker>
    <marker id="aS" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="#64748b"/></marker>
    <style>
      .t {{ fill: #1f2937; font-size: 11.5px; }}
      .s {{ fill: #374151; font-size: 11px; }}
      .ht {{ fill: #fff; font-size: 12.5px; font-weight: 700; letter-spacing: 0.5px; }}
      .m {{ font-family: 'JetBrains Mono', 'DejaVu Sans Mono', monospace; }}
      .lbl {{ font-size: 11px; font-weight: 600; }}
      .sec {{ fill: #0f172a; font-size: 15px; font-weight: 700; }}
    </style>
  </defs>
  <rect width="{W}" height="{H}" fill="#fafafa"/>
  <text x="20" y="30" font-size="20" font-weight="700" fill="#0f172a">Out-of-order core — the life of one instruction</text>
  <text x="20" y="50" class="s">Tomasulo + reorder buffer (32), single issue, 2 CDBs. Instructions enter and leave in program order (dispatch, commit); in between they wait, execute and finish in any order.</text>
''')
stages = [
    ("1 FETCH", "#2aacbe", ["PC → instruction memory", "  (2 cycles, or L1I cache)", "BTB + BHT predict the next", "  PC: taken → target, else +4", "→ instruction queue (8)", "stall: queue full"]),
    ("2 DECODE", "#2aacbe", ["opcode → which unit:", "  ALU · MUL · BR · LSU", "fields rd, rs1, rs2, imm", "CSR / FENCE / ECALL:", "  wait for an empty ROB"]),
    ("3 RENAME · DISPATCH", "#0e7490", ["alloc ROB entry (tail, 32)", "  + RS / LSQ entry", "each source via the RAT:", "  RF → value", "  ROB done → value", "  else → wait on its tag", "RAT[rd] ← this ROB tag", "stall: ROB / RS / LSQ full"]),
    ("4 WAIT IN RS", "#b45309", ["sit in the reservation", "  station until both", "  operands are values", "watch both CDBs: tag", "  match → take the value", "(RAW hazards wait here;", "  WAR / WAW are gone —", "  renaming)"]),
    ("5 ISSUE", "#0e7490", ["each unit picks the", "  oldest ready entry", "→ a free unit:", "  ALU ×2, MUL (pipelined),", "  BRU, LSU (AGU)", "free the RS entry", "out of program order!"]),
    ("6 EXECUTE", "#0e7490", ["ALU, BRU: 1 cycle", "MUL: 3 cycles", "load: address 1 + memory", "  (2 cycles or L1D→L2)", "  or forward from an older", "  store in the LSQ", "branch: actual vs predicted", "  wrong → FLUSH NOW"]),
    ("7 WRITEBACK (CDB ×2)", "#d97706", ["result + ROB tag on a", "  common data bus", "2 per cycle, oldest first", "  (a third waits a cycle)", "→ waiting RS entries wake", "→ ROB entry: value, done", "(stores: done when address", "  and data are known)"]),
    ("8 COMMIT", "#15803d", ["only the ROB head, in", "  program order, 1 / cycle", "done → RF[rd] ← value", "  RAT → RF if still this tag", "store → write memory now", "exception → trap (precise)", "  flush all, PC ← mtvec"]),
]
bw, gap, x0, y0, bh = 172, 13, 20, 72, 236
bx = lambda k: x0 + k * (bw + gap)
for k, (name, col, lines) in enumerate(stages):
    x = bx(k)
    a(f'  <rect x="{x}" y="{y0}" width="{bw}" height="{bh}" rx="4" fill="#fff" stroke="{col}" stroke-width="1.8"/>')
    a(f'  <rect x="{x}" y="{y0}" width="{bw}" height="24" rx="4" fill="{col}"/><text x="{x + 8}" y="{y0 + 17}" class="ht">{E(name)}</text>')
    for j, l in enumerate(lines):
        bold = ' font-weight="700"' if l.startswith('out of') or 'FLUSH' in l else ''
        red = ' style="fill:#dc2626"' if 'FLUSH' in l else ''
        a(f'  <text x="{x + 8}" y="{y0 + 44 + j * 22}" class="t"{bold}{red}>{E(l).replace("  ", "&#160;&#160;")}</text>')
    if k < 7:
        a(f'  <path d="M{x + bw},{y0 + 40} H{x + bw + gap}" stroke="#334155" stroke-width="2" fill="none" marker-end="url(#a)"/>')
def band(x1, x2, y, txt, col, left=False):
    a(f'  <path d="M{x1},{y} H{x2}" stroke="{col}" stroke-width="3"/><path d="M{x1},{y - 6} V{y + 6} M{x2},{y - 6} V{y + 6}" stroke="{col}" stroke-width="3"/>')
    tx, anc = (x1 + 10, "start") if left else ((x1 + x2) / 2, "middle")
    a(f'  <text x="{tx}" y="{y - 6}" text-anchor="{anc}" class="lbl" fill="{col}">{E(txt)}</text>')
band(bx(0), bx(2) + bw, y0 + bh + 22, "IN ORDER — front end", "#0f766e")
band(bx(3), bx(6) + bw, y0 + bh + 22, "OUT OF ORDER — as soon as operands and a unit are ready", "#b45309", left=True)
band(bx(7), bx(7) + bw, y0 + bh + 22, "IN ORDER — commit", "#15803d")
yb = y0 + bh + 40
# CDB wake-up
a(f'  <path d="M{bx(6) + bw / 2},{y0 + bh} V{yb + 30} H{bx(3) + bw / 2} V{y0 + bh + 28}" stroke="#d97706" stroke-width="2.5" fill="none" marker-end="url(#aO)"/>')
a(f'  <text x="{bx(3) + bw / 2 + 10}" y="{yb + 26}" class="lbl" fill="#b45309">CDB broadcasts wake up waiting instructions (tag match, either bus)</text>')
# commit frees ROB
a(f'  <path d="M{bx(7) + 18},{y0 + bh} V{yb + 60} H{bx(2) + bw / 2} V{y0 + bh + 28}" stroke="#15803d" stroke-width="2.5" fill="none" marker-end="url(#aG)"/>')
a(f'  <text x="{bx(3) + 20}" y="{yb + 56}" class="lbl" fill="#15803d">commit frees the ROB entry → dispatch can continue · RAT entry back to the register file</text>')
# mispredict at execute
a(f'  <path d="M{bx(5) + bw - 20},{y0 + bh} V{yb + 90} H{bx(0) + bw / 2 + 12} V{y0 + bh + 28}" stroke="#dc2626" stroke-width="2.5" stroke-dasharray="8 4" fill="none" marker-end="url(#aR)"/>')
a(f'  <text x="{bx(1) + 10}" y="{yb + 86}" class="lbl" fill="#dc2626">mispredict found at execute: flush every younger entry (IQ, RS, LSQ, ROB), rebuild the RAT, fetch from the actual target</text>')
# exception at commit
a(f'  <path d="M{bx(7) + bw - 8},{y0 + bh} V{yb + 112} H{bx(0) + bw / 2 - 12} V{y0 + bh + 28}" stroke="#991b1b" stroke-width="2" stroke-dasharray="3 3" fill="none" marker-end="url(#aR)"/>')
a(f'  <text x="{bx(1) + 10}" y="{yb + 108}" class="lbl" fill="#991b1b">exception at the ROB head: flush everything, trap → mtvec (precise)</text>')
# stall
a(f'  <path d="M{bx(2) + 30},{y0 + bh} V{yb + 8} H{bx(0) + bw - 30} V{y0 + bh + 28}" stroke="#64748b" stroke-width="1.8" stroke-dasharray="3 3" fill="none" marker-end="url(#aS)"/>')
a(f'  <text x="{bx(0) + bw - 20}" y="{yb + 4}" class="lbl" fill="#475569">ROB / RS / LSQ full → stall</text>')

hy = yb + 138
a(f'  <text x="20" y="{hy}" class="sec">How each hazard is handled</text>')
haz = [("RAW (read after write)", "the reader holds the producer's ROB tag and waits in its RS until a CDB broadcasts that tag"),
       ("WAR, WAW", "gone: every result gets its own ROB entry (renaming), so a later write never touches an earlier reader's value"),
       ("structural", "RS / LSQ / ROB full → dispatch stalls; 2 ALUs; MUL is pipelined; 2 CDBs → up to 2 results per cycle, oldest first"),
       ("control (branches)", "predicted at fetch (BHT + BTB); younger instructions run speculatively; a mispredict is flushed as soon as the branch executes"),
       ("memory", "loads wait until all older store addresses are known; same address → take the store's data (forwarding); stores write at commit"),
       ("exceptions / traps", "marked in the ROB entry, taken only at the head → precise: older instructions done, younger never committed")]
for j, (h, t) in enumerate(haz):
    y = hy + 22 + j * 21
    a(f'  <rect x="20" y="{y - 14}" width="1460" height="19" fill="{"#f1f5f9" if j % 2 == 0 else "#fafafa"}"/>')
    a(f'  <text x="28" y="{y}" class="t" font-weight="700">{E(h)}</text><text x="200" y="{y}" class="t">{E(t)}</text>')

ty = hy + 22 + len(haz) * 21 + 28
ncyc = max(r['CM'] for r in rows)
a(f'  <text x="20" y="{ty}" class="sec">Example: cycle by cycle</text>')
a(f'  <text x="250" y="{ty}" class="s">load and MUL take 3 cycles; ALU 1 · 2 CDB broadcasts per cycle (oldest first) · a waiting instruction can issue the cycle after its value is broadcast</text>')
gx, rh = 330, 26
cw = min(80, (W - 30 - gx) // ncyc)
top = ty + 14
a(f'  <text x="28" y="{top + 18}" class="t" font-weight="700">#  instruction</text><text x="250" y="{top + 18}" class="t" font-weight="700">unit</text>')
for c in range(1, ncyc + 1):
    a(f'  <text x="{gx + (c - 1) * cw + cw / 2}" y="{top + 18}" text-anchor="middle" class="t" font-weight="700">{c}</text>')
colors = {"IF": "#94a3b8", "DI": "#0f766e", "RS": "#fde68a", "EX": "#0891b2", "CDB": "#fdba74", "WB": "#d97706", "CM": "#15803d", "ROB": "#e2e8f0"}
for j, r in enumerate(rows):
    y = top + 26 + j * rh
    a(f'  <rect x="20" y="{y}" width="{gx - 20 + ncyc * cw}" height="{rh - 2}" fill="{"#f8fafc" if j % 2 == 0 else "#fff"}"/>')
    a(f'  <text x="28" y="{y + 17}" class="t m">{j + 1}&#160;&#160;{E(r["n"]).replace("  ", "&#160;&#160;")}</text><text x="250" y="{y + 17}" class="t">{r["u"]}</text>')
    cells = {r['IF']: "IF", r['DI']: "DI"}
    for c in range(r['DI'] + 1, r['IS']): cells[c] = "RS"
    for c in range(r['IS'], r['EX'] + 1): cells[c] = "EX"
    if r['WB']:
        for c in range(r['EX'] + 1, r['WB']): cells[c] = "CDB"
        cells[r['WB']] = "WB"
    last = r['WB'] or r['EX']
    for c in range(last + 1, r['CM']): cells[c] = "ROB"
    cells[r['CM']] = "CM"
    for c, k in cells.items():
        x = gx + (c - 1) * cw + 2
        txt = {"RS": "wait", "CDB": "CDB busy", "ROB": "wait head"}.get(k, ("issue+EX" if c == r['IS'] else "EX") if k == "EX" else k)
        dark = k in ("DI", "EX", "WB", "CM", "IF")
        a(f'  <rect x="{x}" y="{y + 1}" width="{cw - 4}" height="{rh - 4}" rx="3" fill="{colors[k]}"/><text x="{x + (cw - 4) / 2}" y="{y + 16}" text-anchor="middle" font-size="10.5" font-weight="700" fill="{"#fff" if dark else "#374151"}">{txt}</text>')
ny = top + 26 + len(rows) * rh + 20
r3, r4 = rows[2], rows[3]
notes = [f"• #4 addi executes in cycle {r4['IS']} and broadcasts in {r4['WB']} — before the older #3 add (EX {r3['IS']}, WB {r3['WB']}), which waits for x4 from the load",
         f"• two CDBs: #2 mul and #4 addi broadcast in the same cycle ({rows[1]['WB']}), nobody waits for the bus",
         "• commit stays in program order " + ", ".join(str(r['CM']) for r in rows) + " — the register file only sees the program's order",
         f"• the store #6 writes memory at its commit ({rows[5]['CM']}), never speculatively"]
for j, l in enumerate(notes):
    a(f'  <text x="28" y="{ny + j * 19}" class="t">{E(l)}</text>')
lx = 1080
for j, (k, t) in enumerate([("IF", "fetch"), ("DI", "decode + dispatch"), ("RS", "wait in RS"), ("EX", "issue / execute"), ("CDB", "wait for a CDB"), ("WB", "CDB broadcast"), ("ROB", "done, wait for head"), ("CM", "commit")]):
    x = lx + (j % 2) * 200; y = ny - 12 + (j // 2) * 20
    a(f'  <rect x="{x}" y="{y}" width="22" height="14" rx="2" fill="{colors[k]}"/><text x="{x + 30}" y="{y + 11}" class="s">{E(t)}</text>')
a('</svg>')
open(sys.argv[1], 'w').write('\n'.join(out))
print("bottom", ny + len(notes) * 19, file=sys.stderr)
