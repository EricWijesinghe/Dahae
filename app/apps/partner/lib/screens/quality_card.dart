// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';

/// Read-only quality card: the score, its status, the five categories and up to three tips.
/// The API computes everything; the partner cannot change it.
class QualityCardScreen extends StatelessWidget {
  const QualityCardScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return Scaffold(
      appBar: AppBar(title: KeepAllText(t.pQualityScore)),
      body: AsyncView<Json>(
        load: () async => await context.api.get('/partner/quality') as Json,
        builder: (context, data, _) => ListView(
          padding: const EdgeInsets.all(Space.gutter),
          children: [
            _ScoreCard(data: data),
            const SizedBox(height: Space.md),
            _ComponentsCard(data: data),
            const SizedBox(height: Space.md),
            _TipsCard(data: data),
            const SizedBox(height: Space.md),
            KeepAllText(t.pQualityReadOnly, style: context.text.bodySmall),
          ],
        ),
      ),
    );
  }
}

String _statusLabel(BuildContext context, String? status) {
  final t = context.t;
  return switch (status) {
    'ok' => t.pQualityOk,
    'warning' => t.pQualityWarning,
    'paused' => t.pQualityPaused,
    'overridden_by_owner' => t.pQualityOverridden,
    _ => t.pQualityNone,
  };
}

class _ScoreCard extends StatelessWidget {
  const _ScoreCard({required this.data});
  final Json data;

  @override
  Widget build(BuildContext context) {
    final score = data['score'] as num?;
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          KeepAllText(context.t.pQualityScore, style: context.text.labelMedium),
          const SizedBox(height: Space.xs),
          KeepAllText(
            score == null ? context.t.pQualityNone : '${score.toInt()}',
            style: context.text.displaySmall,
          ),
          if (score != null)
            KeepAllText(
              _statusLabel(context, data['status'] as String?),
              style: context.text.bodyMedium,
            ),
        ],
      ),
    );
  }
}

class _ComponentsCard extends StatelessWidget {
  const _ComponentsCard({required this.data});
  final Json data;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final c = data['components'] as Json?;
    if (c == null) return const SizedBox.shrink();
    final rows = [
      (t.pQualityRating, c['rating']),
      (t.pQualityCompletion, c['completion']),
      (t.pQualityOnTime, c['on_time']),
      (t.pQualityReliability, c['reliability']),
      (t.pQualityComplaints, c['complaints']),
    ];
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          KeepAllText(t.pQualityComponents, style: context.text.labelMedium),
          for (final (label, value) in rows) ...[
            const SizedBox(height: Space.sm),
            KeepAllText(
              '$label · ${((value as num) * 100).round()}%',
              style: context.text.bodySmall,
            ),
            const SizedBox(height: Space.xs),
            LinearProgressIndicator(value: value.toDouble()),
          ],
        ],
      ),
    );
  }
}

class _TipsCard extends StatelessWidget {
  const _TipsCard({required this.data});
  final Json data;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final tips = [
      for (final tip in (data['tips'] as List? ?? const [])) tip as String,
    ];
    if (tips.isEmpty) return const SizedBox.shrink();
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          KeepAllText(t.pQualityTips, style: context.text.labelMedium),
          for (final tip in tips) ...[
            const SizedBox(height: Space.sm),
            KeepAllText('· ${_tipText(context, tip)}', style: context.text.bodyMedium),
          ],
        ],
      ),
    );
  }
}

String _tipText(BuildContext context, String tip) {
  final t = context.t;
  return switch (tip) {
    'improve_rating' => t.pTipImproveRating,
    'complete_assigned_jobs' => t.pTipCompleteAssigned,
    'check_in_on_time' => t.pTipCheckInOnTime,
    'avoid_cancellations' => t.pTipAvoidCancellations,
    _ => t.pTipReduceComplaints,
  };
}
