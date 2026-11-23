// Copyright (c) 2026 Eric Wijesinghe <EricWijesinghe@hotmail.com>. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter/material.dart';
import 'package:ui/ui.dart';
import 'package:url_launcher/url_launcher.dart';

import 'api.dart';
import 'async_view.dart';
import 'i18n.dart';
import 'models.dart';

Future<void> _openInApp(Uri uri) async {
  await launchUrl(uri, mode: LaunchMode.inAppBrowserView);
}

/// One published legal page (privacy, terms) from the server's `legal` content, shown as plain text.
/// Without a published text, the website's page (`{api}/{slug}.html`, the same text the site shows) opens
/// in an in-app browser view, so every surface shows one text.
class LegalScreen extends StatefulWidget {
  const LegalScreen({
    super.key,
    required this.api,
    required this.slug,
    required this.title,
    this.open = _openInApp,
  });
  final ApiClient api;
  final String slug;
  final String title;

  /// Opens the website page; replaced in tests.
  final Future<void> Function(Uri uri) open;

  @override
  State<LegalScreen> createState() => _LegalScreenState();
}

class _LegalScreenState extends State<LegalScreen> {
  bool _opened = false;

  Uri get _webPage => Uri.parse('${widget.api.baseUrl}/${widget.slug}.html');

  Future<void> _openWeb() async {
    try {
      await widget.open(_webPage);
    } catch (e) {
      if (mounted) showToast(context, context.t.error(e));
    }
  }

  /// The first time the screen finds no text it opens the page by itself; the button opens it again.
  Widget _fallback(BuildContext context) {
    if (!_opened) {
      _opened = true;
      WidgetsBinding.instance.addPostFrameCallback((_) => _openWeb());
    }
    return EmptyState(
      icon: Icons.description_rounded,
      title: context.t.noContent,
      action: OutlinedButton(
        onPressed: _openWeb,
        child: KeepAllText(context.t.rbLegalOpenWeb),
      ),
    );
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: KeepAllText(widget.title)),
    body: AsyncView<List<ContentItem>>(
      load: () async =>
          (await widget.api.get('/content', query: {'kind': 'legal'}) as List)
              .map((e) => ContentItem(e as Json))
              .where((c) => c.j['slug'] == widget.slug)
              .toList(),
      builder: (context, items, _) => items.isEmpty
          ? _fallback(context)
          : ListView(
              padding: const EdgeInsets.all(Space.gutter),
              children: [
                SelectableText(
                  items.first.text(context.lang),
                  style: context.text.bodyMedium,
                ),
              ],
            ),
    ),
  );
}
