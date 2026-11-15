def load(p):
    s=open(p,'rb').read().decode('utf-8'); return s.replace('\r\n','\n'), ('\r\n' in s)
def save(p,s,crlf):
    open(p,'wb').write((s.replace('\n','\r\n') if crlf else s).encode('utf-8'))

p='docs/plans/0.5-depth.md'
s,crlf=load(p)
s+='''
## Fourth pass (2026-11-15, same sitting)
- **Carry a charge** (`feat/charge-carry`): while the Heart's body can be hurt (VALVE,
  OVERDRIVE) a charge appears far from it (`World::fork`). Carried into the ring around the
  Heart (`FORK_DELIVER_R`) it removes `FORK_SHARE` of the phase at once; a hit knocks it loose
  where the player stands. This is the "carry a charge" side objective from the first Heart
  proposal. Results now list grazes and phases broken early.
- **Milestones** (`feat/milestones`): twelve goals kept across runs in the scores file
  (`ACH|id` keys), announced when earned, listed on a sixth Codex page.
- **Live pulses are announced** (`feat/live-predict`): reactive live mode (no second output
  device) used to fire pulses the instant a kick was heard, with no warning. With a confident
  beat (`live_beat.conf > 0.4`) the app now predicts the next beat and announces its pulse a
  full warning ahead (`LiveGen::on_predicted_beat`, `make_pulse`); kicks stop triggering pulses
  while that holds. Not run against real audio here.
'''
save(p,s,crlf)

p='PROJECT_STATE.md'
s,crlf=load(p)
def rep(a,b):
    global s
    assert s.count(a)==1,(a[:60],s.count(a))
    s=s.replace(a,b)
rep('''36. `docs/third-pass-t7` — plan progress, README and this file
''','''36. `docs/third-pass-t7` — plan progress, README and this file
37. `feat/charge-carry` — carry a charge into the Heart; fuller results
38. `feat/milestones` — twelve milestones, sixth Codex page
39. `feat/live-predict` — reactive live mode announces pulses from the predicted beat
40. `docs/fourth-pass-t8` — plan progress, README and this file
''')
rep('''`#UserTesting\\Heartline-0.3.0-T7-win-x64` (built from branch 36).''','''`#UserTesting\\Heartline-0.3.0-T8-win-x64` (built from branch 40).''')
rep('''## Session Log
''','''## Session Log
- 2026-11-15 — Fourth pass of 0.5: carry-a-charge objective, milestones, live pulses announced
  from the predicted beat. 39 unit tests pass; shipped `Heartline-0.3.0-T8`.
''')
save(p,s,crlf)

p='README.md'
s,crlf=load(p)
rep('''- **Resonance.**''','''- **Carry the charge.** While the Heart's body can be hurt, a charge appears far from it. Walk
  it into the ring around the Heart to take a fifth of the phase at once; a hit knocks it loose.
- **Resonance.**''')
rep('''- **CODEX**: what every Heart form, enemy and upgrade does, how the music becomes the level, and
  your best scores.''','''- **CODEX**: what every Heart form, enemy and upgrade does, how the music becomes the level,
  your best scores, and twelve milestones to earn.''')
save(p,s,crlf)
