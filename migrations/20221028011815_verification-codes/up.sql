begin;

create table pin.verification_codes
(
    id       bigint primary key   default pin.id_gen(),
    user_id  bigint      not null references pin.users (id) on delete cascade,
    salt     text        not null,
    hash     text        not null,
    deleted  boolean     not null default false,
    created  timestamptz not null default now(),
    modified timestamptz not null default now()
);
create index idx_verification_codes_user on pin.verification_codes (user_id)
    where deleted is false;
create index idx_verification_codes_created on pin.verification_codes (created)
    where deleted is false;

commit;