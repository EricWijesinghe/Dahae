// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:convert';

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:ui/ui.dart';

ApiClient _api(List<Object?> content) => ApiClient(
  baseUrl: 'https://shop.test',
  session: Session(namespace: 'legal-screen-test'),
  client: MockClient(
    (_) async => http.Response(
      jsonEncode(content),
      200,
      headers: {'content-type': 'application/json'},
    ),
  ),
);

Widget _host(ApiClient api, List<Uri> opened) => MaterialApp(
  theme: buildTheme(BrandConfig.placeholder, Brightness.light),
  locale: const Locale('ko'),
  supportedLocales: const [Locale('ko'), Locale('en')],
  localizationsDelegates: const [
    T.delegate,
    GlobalMaterialLocalizations.delegate,
    GlobalWidgetsLocalizations.delegate,
    GlobalCupertinoLocalizations.delegate,
  ],
  home: LegalScreen(
    api: api,
    slug: 'privacy',
    title: '개인정보',
    open: (u) async => opened.add(u),
  ),
);

void main() {
  testWidgets(
    'without a published text the website page opens, once, with a button to open it again',
    (tester) async {
      final opened = <Uri>[];
      await tester.pumpWidget(_host(_api(const []), opened));
      await tester.pumpAndSettle();

      expect(opened.map((u) => '$u'), ['https://shop.test/privacy.html']);
      await tester.pumpWidget(_host(_api(const []), opened));
      await tester.pumpAndSettle();
      expect(opened, hasLength(1), reason: 'a rebuild does not open it again');

      await tester.tap(find.text('웹 페이지로 보기'));
      await tester.pumpAndSettle();
      expect(opened, hasLength(2));
    },
  );

  testWidgets('a published text is shown and nothing opens', (tester) async {
    final opened = <Uri>[];
    await tester.pumpWidget(
      _host(
        _api([
          {
            'id': 'c1',
            'kind': 'legal',
            'slug': 'privacy',
            'title': {'ko': '개인정보'},
            'body': {'text': 'PRIVACY-TEXT'},
          },
        ]),
        opened,
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('PRIVACY-TEXT'), findsOneWidget);
    expect(opened, isEmpty);
  });
}
