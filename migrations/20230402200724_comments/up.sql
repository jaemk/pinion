begin;

create table pin.comments
(
    id           bigint primary key   default pin.id_gen(),
    pinion_id    bigint      not null references pin.pinions (id),
    user_id      bigint      not null references pin.users (id),
    content      text        not null,
    deleted      boolean     not null default false,
    created      timestamptz not null default now(),
    modified     timestamptz not null default now()
);

create index idx_comment_pinion on pin.comments (pinion_id);
create index idx_comment_user on pin.comments (user_id);
create index idx_comment_created on pin.comments (created);

commit;
