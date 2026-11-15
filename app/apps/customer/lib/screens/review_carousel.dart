import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import 'flow_widgets.dart';
import 'reviews.dart';

const _reviewCardHeight = 132.0;
const _reviewPhotoHeight = 96.0;

/// "생생한 이용 후기" (design p14): recent public reviews; hidden when there are none.
class ReviewCarousel extends StatefulWidget {
  const ReviewCarousel({super.key});
  @override
  State<ReviewCarousel> createState() => _ReviewCarouselState();
}

class _ReviewCarouselState extends State<ReviewCarousel> {
  // Fetched once; a failure only hides the section (social proof is optional).
  late final Future<Object?> _proof = context.api
      .get('/public/proof')
      .then<Object?>((v) => v, onError: (_) => null);

  /// Both photos, or none: half a comparison says nothing.
  static (String, String)? _photos(Json r) {
    final before = r['before_url'];
    final after = r['after_url'];
    return before is String &&
            after is String &&
            before.startsWith('https://') &&
            after.startsWith('https://')
        ? (before, after)
        : null;
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<Object?>(
    future: _proof,
    builder: (context, snap) {
      final reviews = ((snap.data as Json?)?['reviews'] as List? ?? const [])
          .cast<Json>();
      if (reviews.isEmpty) return const SizedBox.shrink();
      final t = context.t;
      final withPhotos = reviews.any((r) => _photos(r) != null);
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const FlowBand(),
          FlowSection(
            title: t.dfReviewsTitle,
            children: [
              Align(
                alignment: AlignmentDirectional.centerEnd,
                child: TextButton(
                  onPressed: () => Navigator.of(context).push(
                    MaterialPageRoute<void>(
                      builder: (_) => const ReviewsAllScreen(),
                    ),
                  ),
                  child: Text(t.reviewsSeeAll),
                ),
              ),
              SizedBox(
                height:
                    _reviewCardHeight + (withPhotos ? _reviewPhotoHeight : 0),
                child: ListView.separated(
                  scrollDirection: Axis.horizontal,
                  itemCount: reviews.length,
                  separatorBuilder: (_, _) => const SizedBox(width: Space.sm),
                  itemBuilder: (context, i) {
                    final r = reviews[i];
                    final photos = _photos(r);
                    return StaggerIn(
                      index: i,
                      child: SizedBox(
                        width: 240,
                        child: AppCard(
                          padding: EdgeInsets.zero,
                          level: Level.flat,
                          border: context.colors.line,
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              if (photos != null)
                                ClipRRect(
                                  borderRadius: const BorderRadius.vertical(
                                    top: Radius.circular(Radii.card),
                                  ),
                                  child: SizedBox(
                                    height: _reviewPhotoHeight,
                                    child: Row(
                                      children: [
                                        Expanded(
                                          child: _Shot(photos.$1, t.dfBefore),
                                        ),
                                        Expanded(
                                          child: _Shot(
                                            photos.$2,
                                            t.dfAfter,
                                            after: true,
                                          ),
                                        ),
                                      ],
                                    ),
                                  ),
                                ),
                              Expanded(
                                child: Padding(
                                  padding: const EdgeInsets.all(Space.md),
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      _ReviewHeader(api: context.api, row: r),
                                      const SizedBox(height: Space.xs),
                                      Expanded(
                                        child: Text(
                                          r['body'] as String? ?? '',
                                          style: context.text.bodySmall,
                                          overflow: TextOverflow.fade,
                                        ),
                                      ),
                                    ],
                                  ),
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    );
                  },
                ),
              ),
            ],
          ),
        ],
      );
    },
  );
}

/// Stars, masked name and the overflow menu (report) of one carousel card.
class _ReviewHeader extends StatelessWidget {
  const _ReviewHeader({required this.api, required this.row});

  final ApiClient api;
  final Json row;

  Future<void> _report(BuildContext context) async {
    final sent = await showReviewReport(
      context,
      api: api,
      bookingId: row['booking_id'] as String,
    );
    if (sent && context.mounted) showToast(context, context.t.reportThanks);
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final c = context.colors;
    return Row(
      children: [
        RatingStars(rating: (row['rating'] as num?)?.toDouble() ?? 5, size: 12),
        const SizedBox(width: Space.xs),
        Expanded(
          child: Text(
            row['name'] as String? ?? '',
            style: context.text.labelMedium?.copyWith(color: c.inkMuted),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ),
        if (row['booking_id'] is String)
          PopupMenuButton<String>(
            tooltip: t.reviewMenu,
            padding: EdgeInsets.zero,
            icon: Icon(Icons.more_vert_rounded, size: 18, color: c.inkMuted),
            onSelected: (_) => _report(context),
            itemBuilder: (_) => [
              PopupMenuItem<String>(
                value: 'report',
                child: Text(t.reportAction),
              ),
            ],
          ),
      ],
    );
  }
}

/// One half of a review's BEFORE | AFTER split (design p14). A photo that fails to load
/// leaves the labelled tint, as in the design's placeholder.
class _Shot extends StatelessWidget {
  const _Shot(this.url, this.label, {this.after = false});
  final String url;
  final String label;
  final bool after;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Stack(
      fit: StackFit.expand,
      children: [
        ColoredBox(color: after ? c.primaryTint : c.surfaceHigh),
        Image.network(
          url,
          fit: BoxFit.cover,
          semanticLabel: label,
          errorBuilder: (_, _, _) => const SizedBox.shrink(),
        ),
        Align(
          alignment: AlignmentDirectional.bottomStart,
          child: Container(
            margin: const EdgeInsets.all(6),
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
            decoration: BoxDecoration(
              color: c.surface.withValues(alpha: 0.9),
              borderRadius: BorderRadius.circular(6),
            ),
            child: ExcludeSemantics(
              child: Text(
                label,
                style: context.text.labelSmall?.copyWith(
                  color: after ? context.primary : c.inkSoft,
                  fontWeight: FontWeight.w800,
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}
