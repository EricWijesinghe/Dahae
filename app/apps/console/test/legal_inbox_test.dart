// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/screens/legal_inbox.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

const _termsHtml =
    '\n    <h1>서비스 이용약관</h1>\n    <p class="note">시행일: 2026년 11월 17일</p>'
    '\n    <h2>사업자 정보</h2><p>상호 주식회사 다해클린 &amp; 파트너</p>';

Json _doc(
  String doc,
  String title, {
  bool pending = false,
  bool published = true,
  String version = '2026-11-17',
}) => {
  'doc': doc,
  'lang': 'ko',
  'title': title,
  'pending': pending,
  'template': {
    'doc': doc,
    'lang': 'ko',
    'title': title,
    'version': version,
    'effective_from': version,
    'requires_reconsent': true,
    'law_refs': ['법'],
    'reviewed_by': 'Dahae',
  },
  'draft_html': _termsHtml,
  'published': published
      ? {
          'id': 1,
          'version': '2026-11-17',
          'effective_at': '2026-11-17',
          'published_at': '2026-11-28T09:00:00Z',
          'body_html': _termsHtml,
        }
      : null,
  'versions': published
      ? [
          {
            'id': 1,
            'version': '2026-11-17',
            'published_at': '2026-11-28T09:00:00Z',
          },
        ]
      : <Json>[],
};

/// Today's Korean install after the first boot of this release: the three texts, published, nothing pending.
Json _kr() => {
  'country': 'KR',
  'documents': [
    _doc('terms', '서비스 이용약관'),
    _doc('privacy', '개인정보처리방침'),
    _doc('location_terms', '위치기반서비스 이용약관'),
  ],
};

Future<void> _pump(WidgetTester tester, FakeApi api) async {
  setViewport(tester, const Size(900, 2400));
  final env = await TestEnv.create(fake: api, role: 'staff_admin');
  await pumpConsoleScreen(
    tester,
    env,
    ListView(children: const [LegalInboxCard()]),
  );
  await settle(tester);
}

void main() {
  testWidgets(
    'a Korean install lists its three texts as published, with nothing to publish',
    (tester) async {
      final api = FakeApi()..reply('GET', '/admin/legal', _kr());
      await _pump(tester, api);

      expect(find.text(koT.lgiTitle), findsOneWidget);
      expect(find.text('서비스 이용약관 · KO'), findsOneWidget);
      expect(find.text('개인정보처리방침 · KO'), findsOneWidget);
      expect(find.text('위치기반서비스 이용약관 · KO'), findsOneWidget);
      expect(
        find.text(koT.lgiPublished('2026-11-17', '2026-11-28')),
        findsNWidgets(3),
      );
      expect(find.text(koT.lgiPending), findsNothing);
      expect(find.widgetWithText(FilledButton, koT.lgiPublish), findsNothing);
      await unmount(tester);
    },
  );

  testWidgets('preview shows the published text as plain text', (tester) async {
    final api = FakeApi()..reply('GET', '/admin/legal', _kr());
    await _pump(tester, api);

    await tester.tap(find.widgetWithText(OutlinedButton, koT.lgiPreview).first);
    await settle(tester);
    expect(find.textContaining('상호 주식회사 다해클린 & 파트너'), findsOneWidget);
    expect(find.textContaining('<p'), findsNothing);
    await tester.tap(find.text(koT.close));
    await settle(tester);
    await unmount(tester);
  });

  testWidgets('versions open one published version by id', (tester) async {
    final api = FakeApi()
      ..reply('GET', '/admin/legal', _kr())
      ..reply('GET', '/admin/legal/versions/1', {
        'id': 1,
        'version': '2026-11-17',
        'body_html': '<p>첫 버전 본문</p>',
      });
    await _pump(tester, api);

    await tester.tap(
      find.widgetWithText(OutlinedButton, koT.lgiVersions).first,
    );
    await settle(tester);
    await tester.tap(find.text('2026-11-17').last);
    await settle(tester);
    expect(api.callsTo('GET', '/admin/legal/versions/1'), hasLength(1));
    expect(find.textContaining('첫 버전 본문'), findsOneWidget);
    await unmount(tester);
  });

  testWidgets(
    'a new text is marked and published per language only after confirming',
    (tester) async {
      var published = false;
      final api = FakeApi()
        ..onGet(
          '/admin/legal',
          (_) => {
            'country': 'KR',
            'documents': [
              if (published)
                _doc('privacy', '개인정보처리방침')
              else
                _doc(
                  'privacy',
                  '개인정보처리방침',
                  pending: true,
                  version: '2026-12-01',
                ),
            ],
          },
        )
        ..onPost('/admin/legal/publish', (_) {
          published = true;
          return {
            'id': 2,
            'doc': 'privacy',
            'lang': 'ko',
            'version': '2026-12-01',
          };
        });
      await _pump(tester, api);

      expect(find.text(koT.lgiPending), findsOneWidget);
      await tester.tap(find.widgetWithText(FilledButton, koT.lgiPublish));
      await settle(tester);
      expect(
        find.textContaining(koT.lgiReconsent),
        findsOneWidget,
        reason: 'the owner is told customers agree again',
      );
      await tester.tap(find.text(koT.cancel));
      await settle(tester);
      expect(
        api.callsTo('POST', '/admin/legal/publish'),
        isEmpty,
        reason: 'cancel publishes nothing',
      );

      await tester.tap(find.widgetWithText(FilledButton, koT.lgiPublish));
      await settle(tester);
      await tester.tap(find.widgetWithText(FilledButton, koT.lgiPublish).last);
      await settle(tester);
      expect(
        api.callsTo('POST', '/admin/legal/publish'),
        isEmpty,
        reason: 'nothing before the owner code',
      );
      expect(find.text(koT.cOwnerCodeTitle), findsOneWidget);
      await tester.enterText(find.byType(TextField).last, '654321');
      await settle(tester);
      await tester.tap(
        find.widgetWithText(FilledButton, koT.cOwnerCodeConfirm),
      );
      await settle(tester);
      final sent = api.lastCall('POST', '/admin/legal/publish')!;
      expect(sent.json, {'doc': 'privacy', 'lang': 'ko'});
      expect(sent.headers['x-owner-totp'], '654321');
      expect(
        find.text(koT.lgiPending),
        findsNothing,
        reason: 'reloaded after publishing',
      );
      expect(api.callsTo('GET', '/admin/legal'), hasLength(2));
      await unmount(tester);
    },
  );

  testWidgets('a server without the inbox shows nothing', (tester) async {
    await _pump(tester, FakeApi());
    expect(find.text(koT.lgiTitle), findsNothing);
    await unmount(tester);
  });

  test('legal HTML becomes readable plain text', () {
    const html =
        '\n    <h1>제목</h1>\n    <p>A &amp; B &lt;b&gt; &#123;x&#125;<br>둘째 줄</p>'
        '\n    <ul><li>하나</li><li>둘</li></ul>'
        '\n    <table><tr><th>구분</th><th>항목</th></tr><tr><td>가</td><td>나</td></tr></table>';
    expect(
      legalPlainText(html),
      '제목\n\nA & B <b> {x}\n둘째 줄\n\n• 하나\n• 둘\n\n구분 | 항목 |\n가 | 나 |',
    );
    expect(
      legalPlainText('&amp;lt;'),
      '&lt;',
      reason: 'an escaped escape stays escaped',
    );
  });
}
