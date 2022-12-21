begin;

create table pin.pinions
(
    id              bigint primary key   default pin.id_gen(),
    user_id         bigint      not null references pin.users (id),
    question_id     bigint      not null references pin.questions (id),
    multi_selection bigint      not null references pin.question_multi_options (id),
    deleted         boolean     not null default false,
    created         timestamptz not null default now(),
    modified        timestamptz not null default now()
);
create index idx_pinions_user on pin.pinions (user_id);
create index idx_pinions_question on pin.pinions (question_id);
create index idx_pinions_multi_selection on pin.pinions (multi_selection);

commit;