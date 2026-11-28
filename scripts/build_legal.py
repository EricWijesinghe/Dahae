# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Generates the website's legal and partner pages from one shared layout.
# Run from the repo root:  python -I scripts/build_legal.py website
# Check only (exit 1 when a page differs from the generator):  python -I scripts/build_legal.py website --check
# Never edit the generated .html by hand: change the text and regenerate. The privacy, terms and location texts are the
# KR country pack's legal templates (backend/packs/KR/legal/<doc>.ko.md, also rendered by the API's Legal inbox); their
# {seller.<field>} slots become the Caddy directives below, {pack.<path>} reads the pack and {doc.effective} the front matter.
#
# Nothing the owner can change in the console is baked in here: deploy/Caddyfile fills it on the server on every
# request. Each page carries {{httpInclude "/v1/public/seller-info?field=..."}} directives (templates on the website
# only), which Caddy replaces with the API's live values (backend routes/seller.rs and routes/seller/live.rs):
# seller details, the business name, colour, icons, absolute URLs, the cancellation rules, retention days, which
# processors are in use, and the legal text published in the console (which replaces the built-in body).
import json
import pathlib
import re
import sys

ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
OUT = pathlib.Path(ARGS[0] if ARGS else pathlib.Path(__file__).parent)
PACKS = pathlib.Path(__file__).resolve().parent.parent / "backend" / "packs"
PACK = "KR"


def seller(field):
    """Caddy directive: the API's value for one field (escaped), or its placeholder until the owner fills it in."""
    return '{{httpInclude "/v1/public/seller-info?field=' + field + '"}}'


NAME = seller("name")

HEAD = """<!doctype html>
<html lang="ko" data-theme="dark">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
  <title>{name} · {title}</title>
  <meta name="description" content="{description}">
  <meta name="theme-color" content="#0B1217">
  <link rel="canonical" href="{base}/{page}">
  <meta property="og:type" content="website">
  <meta property="og:site_name" content="{name}">
  <meta property="og:locale" content="ko_KR">
  <meta property="og:url" content="{base}/{page}">
  <meta property="og:title" content="{name} · {title}">
  <meta property="og:description" content="{description}">
  <meta property="og:image" content="{og}">
  <meta name="twitter:card" content="summary_large_image">
  <link rel="icon" href="{favicon}">
  <link rel="apple-touch-icon" href="{apple}">
  <link rel="manifest" href="site.webmanifest">
  <link rel="preload" href="assets/fonts/DahaeSans-Regular.woff2" as="font" type="font/woff2" crossorigin>
  <link rel="stylesheet" href="assets/site.css">
  <noscript><style>.reveal {{ opacity: 1; transform: none; }}</style></noscript>
  <style>:root {{ --brand: {color}; }}</style>
  <script>try {{ const t = localStorage.getItem('site.theme'); if (t) document.documentElement.dataset.theme = t; }} catch (e) {{}}</script>
  <script defer src="assets/site.js"></script>
</head>
<body>
  <a class="skip" href="#main">본문으로 건너뛰기</a>
  <div class="aurora" aria-hidden="true"><i></i><i></i><i></i></div>
  <header class="nav"><div class="wrap">
    <a class="logo" href="/"><span class="logo-mark" aria-hidden="true">{logo}</span><span data-brand-name>{name}</span></a>
    <div class="tools"><select id="lang" class="lang" aria-label="언어 선택 (Language)"></select><button id="theme" class="icon-btn" type="button" aria-label="밝은 화면으로 보기" title="밝은 화면으로 보기"><span aria-hidden="true">☀️</span></button></div>
  </div></header>
  <main id="main" class="wrap doc">
"""
HEAD_VALUES = {"name": NAME, "base": seller("base_url"), "og": seller("og_image"), "favicon": seller("favicon"),
               "apple": seller("apple_icon"), "color": seller("primary_color"), "logo": seller("logo_img")}

FOOT = """  </main>
  <footer><div class="wrap cols">
    <div><p class="company">{{httpInclude "/v1/public/seller-info"}}</p><p>© <span id="year"></span> <span data-brand-name>""" + NAME + """</span></p><p class="credit">Powered by Dahae · 소프트웨어 © 2026 Eric Wijesinghe</p></div>
    <div><b data-t="f_support">고객센터</b><br>{{httpInclude "/v1/public/seller-info?field=support_phone"}}<br>{{httpInclude "/v1/public/seller-info?field=support_email"}}</div>
    <div><a href="privacy.html" data-t="f_privacy">개인정보처리방침</a><br><a href="terms.html" data-t="f_terms">이용약관</a><br><a href="location-terms.html">위치기반서비스 이용약관</a><br>
      <a href="delete-account.html" data-t="f_delete">계정 삭제 안내</a></div>
  </div></footer>
</body>
</html>
"""

SVC = '<span data-brand-name>' + NAME + '</span>'
CONTACT = seller("support_email") + " · " + seller("support_phone")


def published(field, body):
    """The legal text published in the console (content kind legal) when there is one, else the built-in body."""
    return '{{$o := httpInclude "/v1/public/seller-info?field=' + field + '"}}{{if $o}}{{$o}}{{else}}' + body + '{{end}}\n'


DELETE = f"""
    <h1>계정 삭제 안내 · Delete your account</h1>
    <h2>앱에서 바로 삭제하기</h2>
    <ol><li>{SVC} 앱을 열고 [더보기] 탭으로 이동합니다.</li><li>[계정] → [회원 탈퇴]를 누릅니다.</li><li>안내를 확인하고 [탈퇴하기]를 누르면 계정과 개인정보가 삭제됩니다.</li></ol>
    <h2>앱을 쓸 수 없을 때</h2>
    <p>가입한 휴대전화번호로 고객센터({CONTACT})에 요청하면 본인 확인 후 10일 안에 처리합니다.</p>
    <h2>삭제되는 정보와 보관되는 정보</h2>
    <p>이름·이메일·휴대전화번호, 주소, 자동결제 카드 키, 푸시 토큰, 알림 기록은 삭제됩니다. 상담 대화는 고객을 식별할 수 없도록 연결을 끊고, AI 상담 기록은 보관 기간이 지나면 삭제됩니다. 예약·결제·환불 기록은 전자상거래법에 따라 5년간 따로 보관한 뒤 파기합니다.</p>
    <h2>In English</h2>
    <p>Open the app → More → Account → Delete account. If you cannot use the app, contact support from the phone number you signed up with and we delete the account within 10 days. Payment records are kept for 5 years as Korean e-commerce law requires. Anything still needed for legal retention is kept only as long as the law requires.</p>
"""

PARTNERS = f"""
    <h1>{SVC} 파트너가 되어 주세요</h1>
    <p class="lead">원하는 지역과 시간에 일하고, 완료한 작업은 매주 수요일에 정산받아요.</p>
    <div class="trust" style="margin:32px 0">
      <div class="item"><img class="emoji icon3d" src="assets/icons3d/menu_addresses.webp" alt="" width="64" height="64" loading="lazy" decoding="async"><b>원하는 지역만</b><p>서울 25개 구 가운데 활동 지역을 직접 골라요.</p></div>
      <div class="item"><img class="emoji icon3d" src="assets/icons3d/ui_calendar.webp" alt="" width="64" height="64" loading="lazy" decoding="async"><b>원하는 시간만</b><p>가능한 요일과 시간대를 정하면 맞는 작업만 보여요.</p></div>
      <div class="item"><img class="emoji icon3d" src="assets/icons3d/ui_points.webp" alt="" width="64" height="64" loading="lazy" decoding="async"><b>매주 정산</b><p>완료한 작업은 수수료를 뺀 금액이 매주 수요일에 입금돼요.</p></div>
    </div>
    <h2>지원 방법</h2>
    <ol><li>파트너 앱을 내려받아 휴대전화로 가입합니다.</li><li>팀 이름, 활동 지역, 가능 시간을 입력합니다.</li><li>심사가 끝나면 바로 작업을 받을 수 있어요.</li></ol>
    <div class="store-badges" style="margin-top:24px">
      <a class="store" data-link="partner_app_store_url" role="link" aria-disabled="true"><span><small data-t="store_soon">곧 출시</small><b>App Store</b></span></a>
      <a class="store" data-link="partner_play_store_url" role="link" aria-disabled="true"><span><small data-t="store_soon">곧 출시</small><b>Google Play</b></span></a>
    </div>
"""


def pack_json(pack_id):
    """The pack's JSON with its parents merged under it (objects merge, anything else is replaced), as the API does."""
    data = json.loads((PACKS / pack_id / "pack.json").read_text(encoding="utf-8"))
    parent = data.pop("extends", None)
    return merge(pack_json(parent), data) if parent else data


def merge(base, over):
    if isinstance(base, dict) and isinstance(over, dict):
        return {**base, **{k: merge(base[k], v) if k in base else v for k, v in over.items()}}
    return over


def ko_date(iso):
    year, month, day = (int(p) for p in iso.split("-"))
    return f"{year}년 {month}월 {day}일"


def version_key(version):
    """A document version (YYYY-MM-DD or YYYY-MM-DD.N) as (date, counter), ordered as the API orders it."""
    date, _, counter = version.partition(".")
    return date, int(counter or 0)


def front_matter(path):
    # Front matter between the first two "---" lines (key: value), then the body exactly as the page carries it.
    _, head, body = path.read_text(encoding="utf-8").split("---\n", 2)
    return dict(line.split(": ", 1) for line in head.splitlines()), body


def legal_body(doc):
    """The text customers agree to while nothing is published (the API's legal::agreed_templates), every slot filled
    for the website: the pack template, or the agreed body kept beside it once the template moved past
    privacy.consent_version, so a newer template reaches customers only through the owner's Publish."""
    pack = pack_json(PACK)
    agreed = pack["privacy"]["consent_version"]
    meta, body = front_matter(PACKS / PACK / "legal" / f"{doc}.ko.md")
    if version_key(meta["version"]) > version_key(agreed):
        meta, body = front_matter(PACKS / PACK / "legal" / f"{doc}.ko.{agreed}.md")

    def slot(match):
        space, _, key = match.group(1).partition(".")
        if space == "seller":
            return seller(key)
        if space == "doc" and key == "effective":
            return ko_date(meta["effective_from"])
        if space == "pack":
            value = pack
            for part in key.split("."):
                value = value[part]
            return str(value)
        sys.exit(f"{doc}: unknown slot {match.group(1)}")

    return re.sub(r"\{([a-z_.]+)\}", slot, body)


PRIVACY = legal_body("privacy")
TERMS = legal_body("terms")
LOCATION = legal_body("location_terms")

PAGES = {
    "privacy.html": ("개인정보처리방침", NAME + " 개인정보처리방침: 수집 항목, 이용 목적, 보유 기간, 처리 위탁과 국외 이전, 이용자의 권리.", published("legal_privacy", PRIVACY)),
    "terms.html": ("이용약관", NAME + " 서비스 이용약관: 예약과 결제, 취소와 환불, 이용자의 의무, 분쟁 해결, 사업자 정보.", published("legal_terms", TERMS)),
    "location-terms.html": ("위치기반서비스 이용약관", NAME + " 위치기반서비스 이용약관: 현재 위치로 주소 찾기 기능의 위치정보 처리 기준.", published("legal_location", LOCATION)),
    "delete-account.html": ("계정 삭제 안내", NAME + " 계정 삭제 방법과 삭제·보관되는 정보 안내. How to delete your account.", DELETE),
    "partners.html": ("파트너 지원", NAME + " 청소 파트너 모집: 원하는 지역과 시간에 일하고 매주 수요일 정산받으세요.", PARTNERS),
}

def page_html(name, title, description, body):
    return HEAD.format(title=title, description=description, page=name, **HEAD_VALUES) + body + FOOT


def self_test():
    assert version_key("2026-12-01.10") > version_key("2026-12-01.9") > version_key("2026-12-01"), "not as text"
    page = page_html("terms.html", "t", "d", published("legal_terms", TERMS))
    for field in ("legal_name", "ceo", "registration_no", "mail_order_no", "address", "hosting_provider", "support_phone",
                  "support_email", "name", "base_url", "og_image", "favicon", "apple_icon", "primary_color", "logo_img",
                  "cancel_policy"):
        assert 'httpInclude "/v1/public/seller-info?field=' + field + '"' in page, field
    assert '{{httpInclude "/v1/public/seller-info"}}' in page, "footer block"
    assert 'httpInclude "/v1/public/seller-info?field=bizcheck"' in page, "FTC lookup link comes from the API"
    assert page.count("{{$o := httpInclude") == 1 and page.count("{{end}}") == 1, "one published-text switch per page"
    assert "data-fill" not in page and "data-bizcheck" not in page, "no JavaScript placeholders"
    assert "다해" not in page, "no built-in brand name in the served text"
    privacy = page_html("privacy.html", "t", "d", PRIVACY)
    for field in ("uses_portone", "uses_toss", "uses_solapi", "uses_fcm", "uses_ai", "uses_backup", "uses_juso",
                  "ai_chat_retention_days", "upload_orphan_days", "privacy_email"):
        assert "seller-info?field=" + field + '"' in privacy, field


if __name__ == "__main__":
    check = "--check" in sys.argv
    self_test()
    stale = []
    for name, (title, description, body) in PAGES.items():
        page = page_html(name, title, description, body)
        if not check:
            (OUT / name).write_text(page, encoding="utf-8", newline="\n")
            print("wrote", name)
        elif (OUT / name).read_text(encoding="utf-8").replace("\r\n", "\n") != page:
            stale.append(name)
    if stale:
        sys.exit(f"out of date (edit scripts/build_legal.py, then regenerate): {', '.join(stale)}")
