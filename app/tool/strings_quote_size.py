# Quote address step: note when the chosen layout and the saved address size disagree (C-03).
# Actions: change the layout (back to the 구조 page) or edit the saved size.
COLS = ["ko", "en"]

ROWS = {
  "quoteSizeTitle": ["선택한 구조와 저장된 평수가 달라요", "The layout and the saved size don't match"],
  "quoteSizeBody": ["이 주소는 {size}이라 {layout} 구조와 맞지 않을 수 있어요.", "This address is {size}, which may not suit the {layout} layout."],
  "quoteSizeChangeLayout": ["구조 바꾸기", "Change layout"],
  "quoteSizeChangeSize": ["평수 고치기", "Change size"],
}

for _k, _row in ROWS.items():
    assert len(_row) == len(COLS), _k

S = ROWS
