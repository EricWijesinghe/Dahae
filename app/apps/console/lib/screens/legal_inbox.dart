// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import '../widgets/inline_button.dart';
import 'confirm_action.dart';
import 'setup_owner_code.dart';

/// Legal inbox (`/admin/legal`): each document of the country pack in each language, its published versions and
/// the pack's current text. Nothing applies until the owner presses Publish. A Korean install sees its texts
/// already published, the same as the website. Hidden on a server without the inbox.
class LegalInboxCard extends StatefulWidget {
  const LegalInboxCard({super.key});

  @override
  State<LegalInboxCard> createState() => _LegalInboxCardState();
}

class _LegalInboxCardState extends State<LegalInboxCard> {
  late Future<Json?> _data = _load();

  Future<Json?> _load() async {
    try {
      return await context.api.get('/admin/legal') as Json;
    } on ApiException catch (e) {
      if (e.status == 404) return null;
      rethrow;
    }
  }

  Future<void> _publish(Json doc) async {
    final t = context.t;
    final template = doc['template'] as Json;
    final ask = t.lgiPublishAsk('${doc['title']}', '${template['version']}');
    final ok = await confirmAction(
      context,
      t.lgiPublish,
      message: template['requires_reconsent'] == true
          ? '$ask\n${t.lgiReconsent}'
          : ask,
    );
    if (!ok || !mounted) return;
    // Publishing is the owner's, with the owner's code (the server refuses anyone else).
    final code = await askOwnerCode(context);
    if (code == null || !mounted) return;
    final messenger = ScaffoldMessenger.of(context);
    try {
      await context.api.post(
        '/admin/legal/publish',
        {'doc': doc['doc'], 'lang': doc['lang']},
        null,
        {ownerTotpHeader: code},
      );
      messenger.showSnackBar(
        SnackBar(content: KeepAllText(t.lgiPublishedDone)),
      );
    } on ApiException catch (e) {
      final text = switch (e) {
        _ when e.code == 'totp_invalid' => t.cOwnerCodeWrong,
        _ when e.status == 429 => t.cOwnerCodeLocked,
        _ => t.error(e),
      };
      messenger.showSnackBar(SnackBar(content: KeepAllText(text)));
    }
    if (mounted) {
      setState(() {
        _data = _load();
      });
    }
  }

  Future<void> _versions(Json doc) async {
    final t = context.t;
    final versions = (doc['versions'] as List).cast<Json>();
    final picked = await showDialog<Json>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: KeepAllText('${doc['title']} · ${t.lgiVersions}'),
        content: versions.isEmpty
            ? KeepAllText(t.lgiNoVersions)
            : SizedBox(
                width: 420,
                child: ListView(
                  shrinkWrap: true,
                  children: [
                    for (final v in versions)
                      ListTile(
                        title: KeepAllText('${v['version']}'),
                        subtitle: KeepAllText(_day(v['published_at'])),
                        onTap: () => Navigator.of(ctx).pop(v),
                      ),
                  ],
                ),
              ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(),
            child: KeepAllText(t.close),
          ),
        ],
      ),
    );
    if (picked == null || !mounted) return;
    final Json full;
    try {
      full =
          await context.api.get('/admin/legal/versions/${picked['id']}')
              as Json;
    } on ApiException catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: KeepAllText(t.error(e))));
      }
      return;
    }
    if (mounted) {
      await _preview(
        '${doc['title']} · ${full['version']}',
        '${full['body_html']}',
      );
    }
  }

  Future<void> _preview(String title, String html) => showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: KeepAllText(title),
      content: SizedBox(
        width: 640,
        child: SingleChildScrollView(
          child: SelectableText(legalPlainText(html)),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(ctx).pop(),
          child: KeepAllText(context.t.close),
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) => FutureBuilder<Json?>(
    future: _data,
    builder: (context, snap) {
      final t = context.t;
      if (snap.hasError) return KeepAllText(t.error(snap.error!));
      final data = snap.data;
      if (data == null) return const SizedBox.shrink();
      final docs = (data['documents'] as List).cast<Json>();
      return AppCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            KeepAllText(t.lgiTitle, style: context.text.titleMedium),
            const SizedBox(height: Space.xs),
            KeepAllText(t.lgiIntro, style: context.text.bodySmall),
            for (final d in docs) ...[
              const Divider(height: Space.lg),
              _DocRow(
                key: ValueKey('${d['doc']}.${d['lang']}'),
                doc: d,
                onPreview: () {
                  final published = d['published'] as Json?;
                  // A pending text previews what Publish would apply; otherwise the published one.
                  final html = d['pending'] == true || published == null
                      ? d['draft_html']
                      : published['body_html'];
                  _preview('${d['title']}', '$html');
                },
                onVersions: () => _versions(d),
                onPublish: () => _publish(d),
              ),
            ],
          ],
        ),
      );
    },
  );
}

String _day(Object? iso) =>
    '$iso'.length >= 10 ? '$iso'.substring(0, 10) : '$iso';

class _DocRow extends StatelessWidget {
  const _DocRow({
    super.key,
    required this.doc,
    required this.onPreview,
    required this.onVersions,
    required this.onPublish,
  });

  final Json doc;
  final VoidCallback onPreview;
  final VoidCallback onVersions;
  final VoidCallback onPublish;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final published = doc['published'] as Json?;
    final pending = doc['pending'] == true;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: KeepAllText(
                '${doc['title']} · ${'${doc['lang']}'.toUpperCase()}',
                style: context.text.titleSmall,
              ),
            ),
            if (pending) Chip(label: KeepAllText(t.lgiPending)),
          ],
        ),
        KeepAllText(
          published == null
              ? t.lgiNotPublished
              : t.lgiPublished(
                  '${published['version']}',
                  _day(published['published_at']),
                ),
          style: context.text.bodySmall,
        ),
        const SizedBox(height: Space.xs),
        Wrap(
          spacing: Space.sm,
          runSpacing: Space.xs,
          children: [
            OutlinedButton(
              onPressed: onPreview,
              child: KeepAllText(t.lgiPreview),
            ),
            OutlinedButton(
              onPressed: onVersions,
              child: KeepAllText(t.lgiVersions),
            ),
            if (pending)
              FilledButton(
                style: inlineFilled,
                onPressed: onPublish,
                child: KeepAllText(t.lgiPublish),
              ),
          ],
        ),
      ],
    );
  }
}

final _breaks = RegExp(
  r'<br\s*/?>|</(p|h1|h2|h3|li|tr|ul|ol|table)>',
  caseSensitive: false,
);
final _cells = RegExp(r'</t[dh]>', caseSensitive: false);
final _tags = RegExp(r'<[^>]*>');
final _blankRuns = RegExp(r'\n{3,}');
const _entities = {
  '&lt;': '<',
  '&gt;': '>',
  '&quot;': '"',
  '&#39;': "'",
  '&#123;': '{',
  '&#125;': '}',
  '&amp;': '&',
};

/// A legal text's HTML as readable plain text for the preview: blocks on their own lines, list items bulleted, table
/// cells separated, tags dropped and the escapes the server writes undone (`&amp;` last, so `&amp;lt;` stays `&lt;`).
String legalPlainText(String html) {
  var s = html
      .replaceAll(RegExp(r'<li[^>]*>', caseSensitive: false), '• ')
      .replaceAll(_breaks, '\n')
      .replaceAll(_cells, ' | ')
      .replaceAll(_tags, '');
  _entities.forEach((k, v) => s = s.replaceAll(k, v));
  final lines = s.split('\n').map((l) => l.trim()).join('\n');
  return lines.replaceAll(_blankRuns, '\n\n').trim();
}
