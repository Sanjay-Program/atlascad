import sys, math
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
OUT = REPO + 'doc/'
errors, fails = [], []
def check(name, cond, info=''):
    print(('PASS' if cond else 'FAIL'), name, info)
    if not cond: fails.append(name)
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1600, 'height': 1000})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' else None)
    pg.goto('http://127.0.0.1:8080/?theme=dark&lang=en'); pg.wait_for_timeout(3500)
    st = pg.evaluate('cadrust.status()')
    check('bodies loaded', len(st['bodies']) >= 1, str(len(st['bodies'])))
    vb = pg.evaluate("(() => { const r = document.querySelector('#viewport canvas').getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; })()")
    cx, cy = vb[0] + vb[2] / 2, vb[1] + vb[3] / 2

    # --- marking menu: right click without drag
    pg.mouse.move(cx, cy); pg.mouse.click(cx, cy, button='right'); pg.wait_for_timeout(300)
    mark = pg.evaluate('cadrust.mark()')
    check('marking menu 8 slots', len(mark) == 8, str(mark))
    pg.screenshot(path=OUT + 'ui_marking_menu.png')
    pg.keyboard.press('Escape'); pg.wait_for_timeout(100)
    check('marking menu closes on Esc', pg.evaluate('cadrust.mark()') == [])
    # pick a slot: Fit, which should move to the front of the recent list
    pg.mouse.click(cx, cy, button='right'); pg.wait_for_timeout(200)
    pg.click('#mark .mi[data-id=Undo]'); pg.wait_for_timeout(200)
    check('recent list reorders', pg.evaluate('cadrust.recent()')[0] == 'Undo', str(pg.evaluate('cadrust.recent()')))

    # --- measure: helper between bbox corners, then real clicks
    body = st['bodies'][0]
    lo, hi = body['bbox_min'], body['bbox_max']
    res = pg.evaluate('cadrust.measurePoints(%s, %s)' % (lo, hi))
    exp = math.dist(lo, hi)
    check('measure distance', abs(res['distance'] - exp) < 1e-6, '%.3f vs %.3f' % (res['distance'], exp))
    m = pg.evaluate('cadrust.measure()')
    check('measure panel visible', pg.is_visible('#measure') and len(m['rows']) == 6 and abs(float(m['labels'][0].split()[0]) - exp) < 0.01, str(m['labels']))
    # real click on the model: point 1 + body rows
    pg.mouse.click(cx, cy); pg.wait_for_timeout(300)
    m = pg.evaluate('cadrust.measure()')
    check('click gives point + body rows', any(r[0] == 'Body' for r in m['rows']) and m['rows'][0][0].startswith('Point'), str([r[0] for r in m['rows']]))
    pg.mouse.click(cx + 120, cy - 40); pg.wait_for_timeout(300)
    m = pg.evaluate('cadrust.measure()')
    check('second click gives distance', m['rows'][0][0] == 'Distance', str(m['rows'][:1]))
    pg.screenshot(path=OUT + 'ui_measure.png')
    pg.keyboard.press('Escape'); pg.wait_for_timeout(100)
    check('Esc exits measure', not pg.evaluate('cadrust.measure().on') and not pg.is_visible('#measure'))
    pg.keyboard.press('i'); pg.wait_for_timeout(100)
    check('I toggles measure', pg.evaluate('cadrust.measure().on'))
    pg.keyboard.press('i'); pg.wait_for_timeout(100)

    # --- section view
    pg.click('#btnSection'); pg.wait_for_timeout(300)
    sec = pg.evaluate('cadrust.section()')
    check('section clips all meshes', sec['on'] and sec['clipped'] == sec['meshes'] and sec['meshes'] > 0, str(sec))
    check('section panel visible', pg.is_visible('#secpanel'))
    pg.click('#secpanel [data-ax=x]'); pg.wait_for_timeout(100)
    sec = pg.evaluate('cadrust.section()')
    check('section axis x spans bbox', sec['axis'] == 'x' and abs(sec['lo'] - lo[0]) < 0.1 and abs(sec['hi'] - hi[0]) < 0.1, str((sec['lo'], sec['hi'])))
    pg.screenshot(path=OUT + 'ui_section_view.png')
    # survives a rebuild
    pg.click('#btnRun'); pg.wait_for_timeout(1500)
    sec = pg.evaluate('cadrust.section()')
    check('section survives rebuild', sec['on'] and sec['clipped'] == sec['meshes'], str(sec))
    pg.click('#btnSection'); pg.wait_for_timeout(200)
    sec = pg.evaluate('cadrust.section()')
    check('section off clears planes', not sec['on'] and sec['clipped'] == 0, str(sec))

    # --- display mode cycle
    d0 = pg.evaluate('cadrust.display()')
    check('display default shaded+edges', d0['mode'] == 'edges' and d0['edgesVisible'] and not d0['wire'], str(d0))
    pg.click('#btnDisplay'); pg.click('#dispMenu [data-mode=wire]'); pg.wait_for_timeout(100); d1 = pg.evaluate('cadrust.display()')
    check('display wireframe', d1['mode'] == 'wire' and d1['wire'] and d1['edgesVisible'], str(d1))
    pg.click('#btnDisplay'); pg.click('#dispMenu [data-mode=shaded]'); pg.wait_for_timeout(100); d2 = pg.evaluate('cadrust.display()')
    check('display shaded hides edges', d2['mode'] == 'shaded' and not d2['wire'] and not d2['edgesVisible'], str(d2))
    pg.click('#btnDisplay'); pg.click('#dispMenu [data-mode=edges]'); pg.wait_for_timeout(100)
    check('display cycles back', pg.evaluate('cadrust.display().mode') == 'edges')
    pg.click('#btnGridMenu'); pg.click('#btnGrid'); check('grid toggles off', not pg.evaluate("document.getElementById('btnGrid').classList.contains('active')")); pg.click('#btnGrid'); pg.keyboard.press('Escape')

    # --- hover / selection filter
    pg.mouse.move(cx, cy); pg.wait_for_timeout(150)
    hv = pg.evaluate('cadrust.hovered()')
    check('body hover highlight', hv['mesh'] or hv['edge'], str(hv))
    pg.select_option('#selFilter', 'edges'); pg.mouse.move(cx + 3, cy + 3); pg.wait_for_timeout(150)
    check('edges filter blocks body hover', not pg.evaluate('cadrust.hovered().mesh'))
    pg.select_option('#selFilter', 'bodies'); pg.mouse.click(cx, cy); pg.wait_for_timeout(150)
    check('bodies filter selects body', pg.evaluate('cadrust.hovered().body') is not None, str(pg.evaluate('cadrust.hovered().body')))
    pg.select_option('#selFilter', 'all'); pg.keyboard.press('Escape')

    # --- shortcuts: help overlay, Home, Enter confirms dialog, no shortcut while typing
    pg.keyboard.press('?'); pg.wait_for_timeout(100)
    check('help overlay opens', pg.is_visible('#shelp') and pg.evaluate("document.querySelectorAll('#shelp tr').length") >= 15)
    pg.keyboard.press('Escape'); pg.wait_for_timeout(100)
    check('help overlay closes', not pg.is_visible('#shelp'))
    pg.keyboard.press('Home'); pg.wait_for_timeout(100)
    check('Home noted as Fit in recent', pg.evaluate('cadrust.recent()')[0] == 'Fit')
    n0 = len(pg.evaluate('cadrust.doc().timeline'))
    pg.keyboard.press('m'); pg.wait_for_timeout(200)
    check('M opens Move dialog', pg.is_visible('#dialog') and 'MOVE' in pg.inner_text('#dialog .dh').upper())
    pg.evaluate("document.activeElement.blur()")
    pg.keyboard.press('Enter'); pg.wait_for_timeout(800)
    check('Enter confirms dialog', len(pg.evaluate('cadrust.doc().timeline')) == n0 + 1)
    pg.click('#btnUndo'); pg.wait_for_timeout(800)
    pg.click('#btnData'); pg.click('#adv summary'); pg.focus('#json'); pg.keyboard.press('e'); pg.wait_for_timeout(100)
    check('no shortcut while typing', not pg.is_visible('#dialog'))
    pg.evaluate("document.getElementById('json').value = JSON.stringify(cadrust.doc(), null, 2); document.activeElement.blur()"); pg.click('#adv summary'); pg.click('#btnDataClose')
    tip = pg.get_attribute('button.tool[data-tool=Extrude]', 'title')
    check('tooltip has shortcut', tip == 'Extrude (E)', tip)

    # --- sketch entity list shows solved coordinates, expression kept in muted text
    pg.click('button.tool[data-tool=Sketch]'); pg.wait_for_timeout(300)
    pg.fill('#skOff', '40'); pg.click('#dOk'); pg.wait_for_timeout(600)
    pg.keyboard.press('r')
    for (u, v) in [(70, 0), (100, 20)]:
        x, y = pg.evaluate('cadrust.sketchUV(%s,%s)' % (u, v)); pg.mouse.move(x, y); pg.wait_for_timeout(60); pg.mouse.click(x, y); pg.wait_for_timeout(200)
    pg.wait_for_timeout(800)
    pg.evaluate("(() => { const i = document.querySelector('#ents input[data-k=w]'); i.value = '15*2'; i.dispatchEvent(new Event('change')); })()")
    pg.wait_for_timeout(900)
    shown = pg.evaluate("document.querySelector('#ents input[data-k=w]').value")
    raw = pg.evaluate("(document.querySelector('#ents .raw') || {}).textContent")
    check('entity list solved value', shown == '30', shown)
    check('entity list expression shown', raw == '= 15*2', str(raw))
    pg.click('#btnCancelSketch'); pg.wait_for_timeout(600)

    # --- korean strings for the new UI
    pg.select_option('#lang', 'ko'); pg.wait_for_timeout(200)
    check('ko labels', pg.get_attribute('#btnMeasure', 'title') == '측정 (I)' and pg.text_content('#dispMenu [data-mode=edges]').strip().endswith('가시 모서리가 있는 음영처리') and pg.inner_text('.rlabel[data-menu=create]').strip().startswith('작성'), pg.get_attribute('#btnMeasure', 'title') + '|' + pg.inner_text('.rlabel[data-menu=create]'))
    pg.select_option('#lang', 'en')

    check('no console errors', not errors, '\n'.join(errors[:5]))
    b.close()
print('FAILS', fails)
