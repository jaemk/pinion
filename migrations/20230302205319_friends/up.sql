begin;

create table pin.friends
(
    id           bigint primary key   default pin.id_gen(),
    requestor_id bigint      not null references pin.users (id),
    acceptor_id  bigint      not null references pin.users (id),
    accepted     timestamptz,
    deleted      boolean     not null default false,
    created      timestamptz not null default now(),
    modified     timestamptz not null default now(),
    constraint not_same_user check (requestor_id != acceptor_id)
);
create index idx_friends_requestor on pin.friends (requestor_id);
create index idx_friends_acceptor on pin.friends (acceptor_id);
create unique index idx_friends_unique on pin.friends
    (least(requestor_id, acceptor_id), greatest(requestor_id, acceptor_id))
where deleted is false;

commit;