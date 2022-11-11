begin;

create table pin.users
(
    id       bigint primary key   default pin.id_gen(),
    handle   text not null,
    deleted  boolean     not null default false,
    created  timestamptz not null default now(),
    modified timestamptz not null default now()
);
create unique index idx_users_handle on pin.users (handle)
    where deleted is false;

create table pin.phones
(
    id                    bigint primary key   default pin.id_gen(),
    user_id               bigint      not null references pin.users (id),
    number                text        not null,
    verification_attempts int         not null default 0,
    verification_sent     timestamptz,
    verified              timestamptz,
    deleted               boolean     not null default false,
    created               timestamptz not null default now(),
    modified              timestamptz not null default now()
);
create unique index idx_phones_user on pin.phones (user_id)
    where deleted is false;
create unique index idx_phones_number on pin.phones (number)
    where deleted is false and verified is not null;

create table pin.passwords
(
    id       bigint primary key   default pin.id_gen(),
    user_id  bigint      not null references pin.users (id) on delete cascade,
    salt     text        not null,
    hash     text        not null,
    deleted  boolean     not null default false,
    created  timestamptz not null default now(),
    modified timestamptz not null default now()
);
create index idx_passwords_user on pin.passwords (user_id)
    where deleted is false;

create table pin.auth_tokens
(
    id       bigint primary key   default pin.id_gen(),
    user_id  bigint      not null references pin.users (id) on delete cascade,
    hash     text unique not null,
    version  bigint      not null default 1,
    expires  timestamptz not null,
    deleted  boolean     not null default false,
    created  timestamptz not null default now(),
    modified timestamptz not null default now()
);
create index idx_auth_tokens_user_id on pin.auth_tokens (user_id)
    where deleted is false;
create index idx_auth_tokens_hash on pin.auth_tokens (hash)
    where deleted is false;
create index idx_auth_tokens_expires on pin.auth_tokens (expires)
    where deleted is false;

commit;
