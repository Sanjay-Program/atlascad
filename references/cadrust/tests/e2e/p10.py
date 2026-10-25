"""Responsive ribbon: narrow windows fold low-priority buttons into the group drop-downs; very narrow ones get a scrollbar."""
import sys, json
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
OUT = REPO + 'doc/'
BASE = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:8080/'
errors, fails = [], []
def check(name, cond, info=''):
    print(('PASS' if cond else 'FAIL'), name, info)
    if not cond: fails.append(name)
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1200, 'height': 800})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' and '404' not in m.text else None)
    pg.goto(BASE + '?theme=dark&lang=en'); pg.wait_for_timeout(3500)
    def right_edge_visible():
        return pg.evaluate("(() => { const p = document.querySelector('#tab-solid'), g = p.querySelector('[data-group=addins]').getBoundingClientRect(); return g.right <= p.getBoundingClientRect().right + 1; })()")
    r = pg.evaluate('cadrust.ribbon()')
    check('1200px: ribbon fits by folding buttons', r['overflow'] <= 0 and not r['scrollbar'] and len(r['folded']) > 0, json.dumps(r))
    check('1200px: ADD-INS group reachable', right_edge_visible())
    check('core tools stay visible', pg.is_visible('button.tool[data-tool=Extrude]') and pg.is_visible('button.tool[data-tool=Fillet]') and pg.is_visible('#btnPressPull'))
    check('placeholders fold first', 'tool.NewComponent' in r['folded'], str(r['folded']))
    empty = pg.evaluate("[...document.querySelectorAll('#tab-solid .rgroup')].filter(g => ![...g.querySelectorAll('.rb')].some(b => getComputedStyle(b).display !== 'none')).map(g => g.dataset.group)")
    check('no group is left without an icon', empty == [], str(empty))
    # a folded command is still in its group's drop-down and still works
    folded_tool = next((f for f in r['folded'] if f in ('Mirror', 'Box', 'Loft', 'Sweep', 'Hole')), None)
    check('a create tool was folded', folded_tool is not None, str(r['folded']))
    pg.click('.rlabel[data-menu=create]'); pg.wait_for_timeout(150)
    label = pg.evaluate("t => cadrust.label(t)", folded_tool)
    check('folded tool listed in the drop-down', pg.is_visible(f'#groupMenu .mi:has-text("{label}")'))
    pg.click(f'#groupMenu .mi:has-text("{label}")'); pg.wait_for_timeout(300)
    check('folded tool opens its dialog', pg.is_visible('#dialog') and label.upper() in pg.inner_text('#dialog .dh').upper(), pg.inner_text('#dialog .dh'))
    pg.click('#dCancel')
    pg.screenshot(path=OUT + 'ui_ribbon_1200.png', clip={'x': 0, 'y': 0, 'width': 1200, 'height': 150})
    # wide window restores everything
    pg.set_viewport_size({'width': 1700, 'height': 900}); pg.wait_for_timeout(400)
    r = pg.evaluate('cadrust.ribbon()')
    check('1700px: nothing folded', r['folded'] == [] and r['overflow'] <= 0, json.dumps(r))
    # very narrow: everything foldable is folded and a scrollbar appears
    pg.set_viewport_size({'width': 700, 'height': 800}); pg.wait_for_timeout(400)
    r = pg.evaluate('cadrust.ribbon()')
    check('700px: scrollbar fallback', r['scrollbar'] and r['overflow'] > 0, json.dumps(r))
    check('700px: scrollbar is themed and thin', pg.evaluate("getComputedStyle(document.querySelector('#tab-solid')).scrollbarWidth") == 'thin')
    pg.screenshot(path=OUT + 'ui_ribbon_700.png', clip={'x': 0, 'y': 0, 'width': 700, 'height': 150})
    # sketch tab folds too
    pg.set_viewport_size({'width': 1000, 'height': 800}); pg.wait_for_timeout(300)
    pg.click('button.tool[data-tool=Sketch]'); pg.click('#dOk'); pg.wait_for_timeout(800)
    r = pg.evaluate('cadrust.ribbon()')
    check('sketch tab fits at 1000px', r['overflow'] <= 0, json.dumps(r))
    pg.click('#btnCancelSketch'); pg.wait_for_timeout(500)
    check('no console errors', not errors, '\n'.join(errors[:5]))
    b.close()
print('FAILS', fails)
