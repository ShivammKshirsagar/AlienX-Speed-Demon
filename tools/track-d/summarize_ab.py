import re

with open(r'C:\Users\chawa\.gemini\antigravity\brain\fc1b01f4-5456-4c67-bdf1-2a7d63f1859c\.system_generated\tasks\task-2986.log', 'r') as f:
    text = f.read()

pattern = re.compile(r'^\s*([A-Za-z0-9_ ()]+?)\s*:\s*query=\s*([0-9.]+) ms \|\s*total=\s*([0-9.]+) ms', re.MULTILINE)
matches = pattern.findall(text)

data = {}
for name, q, t in matches:
    name = name.strip()
    data.setdefault(name, {'q': [], 't': []})
    data[name]['q'].append(float(q))
    data[name]['t'].append(float(t))

print("=" * 100)
print(f"{'Variant':<24} | {'12w Query Med':<14} | {'Query Spread':<22} | {'12w Total Med':<14} | {'Total Spread':<22}")
print("-" * 100)
for name, d in data.items():
    qs = sorted(d['q'])
    ts = sorted(d['t'])
    q_med = qs[len(qs)//2]
    t_med = ts[len(ts)//2]
    q_spread = f"[{qs[0]:.1f} - {qs[-1]:.1f}] (+{qs[-1]-qs[0]:.1f})"
    t_spread = f"[{ts[0]:.1f} - {ts[-1]:.1f}] (+{ts[-1]-ts[0]:.1f})"
    print(f"{name:<24} | {q_med:8.2f} ms     | {q_spread:<22} | {t_med:8.2f} ms     | {t_spread:<22}")
print("=" * 100)
