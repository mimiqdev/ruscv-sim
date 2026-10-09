"""Deterministic descriptive/sufficiency statistics, never a speed gate."""
import random
import statistics
from integrity import require
from report_schema import quantile


def distribution(values, policy):
    require(all(type(v) is int and v>=0 for v in values),'nonnegative integer summed sample intervals')
    if not values:
        return {'count':0,'median_ns':None,'p05_ns':None,'p95_ns':None,'mad_ns':None,
                'bootstrap_median_ci':None,'relative_ci_half_width':None,'sufficient_noise':False}
    median=statistics.median(values)
    p=policy['statistics']
    rng=random.Random(p['seed'])
    boot=[statistics.median(rng.choices(values,k=len(values))) for _ in range(p['resamples'])]
    tail=(1-p['confidence'])/2
    low,high=quantile(boot,tail),quantile(boot,1-tail)
    half=(high-low)/(2*median) if median>0 else None
    return {'count':len(values),'median_ns':median,'p05_ns':quantile(values,.05),'p95_ns':quantile(values,.95),
            'mad_ns':statistics.median([abs(x-median) for x in values]),
            'bootstrap_median_ci':{'method':p['bootstrap'],'confidence':p['confidence'],'resamples':p['resamples'],'seed':p['seed'],'low_ns':low,'high_ns':high},
            'relative_ci_half_width':half,'sufficient_noise':half is not None and half<=p['relative_ci_half_width_limit']}


def ratio(base,candidate,policy):
    require(len(base)>=policy['sampling']['accepted_samples'] and len(candidate)>=policy['sampling']['accepted_samples'],'insufficient samples: no ratio')
    require(distribution(base,policy)['sufficient_noise'] and distribution(candidate,policy)['sufficient_noise'],'unstable noise: no ratio')
    p=policy['statistics'];rng=random.Random(p['seed'])
    boot=[statistics.median(rng.choices(candidate,k=len(candidate)))/statistics.median(rng.choices(base,k=len(base))) for _ in range(p['resamples'])]
    tail=(1-p['confidence'])/2
    return {'candidate_over_baseline':statistics.median(candidate)/statistics.median(base),
            'confidence':p['confidence'],'resamples':p['resamples'],'seed':p['seed'],
            'method':'independent percentile bootstrap ratio of medians',
            'low':quantile(boot,tail),'high':quantile(boot,1-tail),
            'classification':'informational; no speed/regression gate'}
