// Copyright (c) 2026 Eric Wijesinghe <EricWijesinghe@hotmail.com>. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import 'api.dart';
import 'async_view.dart';
import 'i18n.dart';
import 'models.dart';

/// One published legal page (privacy, terms) from the server's `legal` content, shown as plain text.
class LegalScreen extends StatelessWidget {
  const LegalScreen({
    super.key,
    required this.api,
    required this.slug,
    required this.title,
  });
  final ApiClient api;
  final String slug;
  final String title;
  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: Text(title)),
    body: AsyncView<List<ContentItem>>(
      load: () async =>
          (await api.get('/content', query: {'kind': 'legal'}) as List)
              .map((e) => ContentItem(e as Json))
              .where((c) => c.j['slug'] == slug)
              .toList(),
      builder: (context, items, _) => items.isEmpty
          ? EmptyState(
              icon: Icons.description_rounded,
              title: context.t.noContent,
            )
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
